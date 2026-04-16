//! Async log forwarding infrastructure.
//!
//! Provides a bounded-channel [`LogForwarder`] that buffers
//! [`LedgerEntry`] values and a background task that batches them for
//! transmission to a remote SIEM/syslog endpoint.
//!
//! REQ-AUDIT-018 -- async forwarding infrastructure via tokio channel
//! REQ-AUDIT-019 -- buffer overflow policy (DropOldest / DropNewest / Block)
//! REQ-AUDIT-020 -- delivery retry with exponential backoff
//!
//! # Design notes
//!
//! * The forwarder exposes `tokio::sync::mpsc::Sender<LedgerEntry>` via
//!   [`LogForwarder::forward`]. The matching receiver is handed to the
//!   caller so the composition root can spawn [`LogForwarder::run`] on
//!   a runtime it controls.
//! * Network I/O is abstracted behind the [`BatchPoster`] trait so
//!   tests can drive retry/backoff semantics without touching the
//!   network. The default transport ([`StubPoster`]) is a no-op that
//!   always succeeds; real HTTP/syslog transports plug in via
//!   [`LogForwarder::run_with_poster`].
//! * Batches failing delivery after `max_retries` are dropped with a
//!   `tracing::warn!` event. We deliberately do not persist a
//!   dead-letter file from this module: the on-disk JSONL ledger
//!   ([`crate::ledger::JsonlLedger`]) is the durable source of truth;
//!   forwarding is best-effort transport layered on top.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};

// ---------------------------------------------------------------------------
// LedgerEntry -- public, owned companion to `ledger::LedgerEntry`.
//
// The private struct in `ledger.rs` borrows `&DomainEvent`, which is ideal
// for the append-only writer but awkward to pass across an mpsc channel.
// Forwarding owns its entries, so we expose an owned variant here.
// ---------------------------------------------------------------------------

/// An audit ledger entry as seen by the forwarder. Mirrors the on-disk
/// JSONL shape produced by [`crate::ledger::JsonlLedger`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LedgerEntry {
    pub timestamp: DateTime<Utc>,
    pub os_user: String,
    pub hostname: String,
    pub event: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub req_id: Option<String>,
}

impl LedgerEntry {
    /// Convenience constructor.
    pub fn new(
        timestamp: DateTime<Utc>,
        os_user: impl Into<String>,
        hostname: impl Into<String>,
        event: serde_json::Value,
        req_id: Option<String>,
    ) -> Self {
        Self {
            timestamp,
            os_user: os_user.into(),
            hostname: hostname.into(),
            event,
            req_id,
        }
    }
}

// ---------------------------------------------------------------------------
// Overflow policy (REQ-AUDIT-019)
// ---------------------------------------------------------------------------

/// How the forwarder behaves when its channel buffer is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverflowPolicy {
    /// Discard the oldest buffered entry to make room for the new one.
    DropOldest,
    /// Return [`ForwardError::BufferFull`] immediately. Default; most
    /// predictable, no extra allocation.
    #[default]
    DropNewest,
    /// Await until buffer space is available.
    Block,
}

// ---------------------------------------------------------------------------
// Config (REQ-AUDIT-018, REQ-AUDIT-020)
// ---------------------------------------------------------------------------

/// Configuration for a [`LogForwarder`].
#[derive(Debug, Clone)]
pub struct ForwarderConfig {
    /// Target endpoint, e.g. `syslog://host:514` or `https://siem/ingest`.
    pub endpoint: String,
    /// Maximum number of entries buffered between producer and batcher.
    pub buffer_size: usize,
    /// Number of entries per outbound batch.
    pub batch_size: usize,
    /// Maximum time between flushes, in milliseconds. A partial batch
    /// is flushed once this interval elapses.
    pub flush_interval_ms: u64,
    /// Behaviour when the buffer is full. See [`OverflowPolicy`].
    pub overflow_policy: OverflowPolicy,
    /// Maximum number of retry attempts for a failing batch. `0` means
    /// no retries (single shot).
    pub max_retries: u32,
    /// Base delay between retries, in milliseconds. Actual delay is
    /// approximately `base_delay_ms * 2^attempt` with a small jitter.
    pub base_delay_ms: u64,
}

impl ForwarderConfig {
    /// Production-sensible defaults: 1024 buffer, 50/batch, 5s flush,
    /// drop-newest on overflow, 3 retries with 500 ms base delay.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            buffer_size: 1024,
            batch_size: 50,
            flush_interval_ms: 5_000,
            overflow_policy: OverflowPolicy::DropNewest,
            max_retries: 3,
            base_delay_ms: 500,
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by [`LogForwarder::forward`].
#[derive(Debug)]
pub enum ForwardError {
    /// The buffer is full and policy is [`OverflowPolicy::DropNewest`].
    BufferFull,
    /// The background [`LogForwarder::run`] task has exited and the
    /// channel is closed.
    ChannelClosed,
}

impl fmt::Display for ForwardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferFull => f.write_str("forwarder buffer full; entry dropped"),
            Self::ChannelClosed => f.write_str("forwarder channel closed"),
        }
    }
}

impl std::error::Error for ForwardError {}

/// Transport-layer error surfaced to the retry loop.
#[derive(Debug)]
pub struct PostError {
    pub message: String,
}

impl PostError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PostError {}

// ---------------------------------------------------------------------------
// Transport trait and default stub
// ---------------------------------------------------------------------------

/// Trait abstracting the network transport used by the forwarder.
///
/// Factoring the transport behind a trait lets tests drive retry/backoff
/// semantics deterministically without linking a full HTTP client, and
/// lets integrations plug in syslog / HTTPS / Kafka transports later
/// (REQ-AUDIT-011, REQ-AUDIT-012).
#[async_trait]
pub trait BatchPoster: Send + Sync {
    /// POST a batch of entries to the configured endpoint. Return `Err`
    /// on transient failure; the forwarder will retry up to
    /// `max_retries` times with exponential backoff.
    async fn post(&self, endpoint: &str, batch: &[LedgerEntry]) -> Result<(), PostError>;
}

/// Default transport: records batch sizes for inspection but performs
/// no network I/O. Useful as a placeholder and for tests.
#[derive(Debug, Default)]
pub struct StubPoster {
    batches: Arc<Mutex<Vec<Vec<LedgerEntry>>>>,
}

impl StubPoster {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a clone of all batches this poster has seen so far.
    pub async fn batches(&self) -> Vec<Vec<LedgerEntry>> {
        self.batches.lock().await.clone()
    }

    /// Total number of entries across all batches.
    pub async fn total_entries(&self) -> usize {
        self.batches.lock().await.iter().map(Vec::len).sum()
    }
}

#[async_trait]
impl BatchPoster for StubPoster {
    async fn post(&self, _endpoint: &str, batch: &[LedgerEntry]) -> Result<(), PostError> {
        self.batches.lock().await.push(batch.to_vec());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LogForwarder (REQ-AUDIT-018)
// ---------------------------------------------------------------------------

/// Bounded channel-based log forwarder.
///
/// Construct with [`LogForwarder::new`], submit entries via
/// [`LogForwarder::forward`], and spawn the batcher via
/// [`LogForwarder::run`] or [`LogForwarder::run_with_poster`].
pub struct LogForwarder {
    tx: mpsc::Sender<LedgerEntry>,
    /// Shared receiver handle used only by [`OverflowPolicy::DropOldest`]
    /// to pop the oldest entry when the buffer is full. `None` for the
    /// other policies.
    drop_oldest_receiver: Option<Arc<Mutex<mpsc::Receiver<LedgerEntry>>>>,
    config: ForwarderConfig,
}

impl LogForwarder {
    /// Build a new forwarder and return the matching receiver. The
    /// caller is responsible for spawning [`Self::run_with_poster`].
    pub fn new(config: ForwarderConfig) -> (Self, mpsc::Receiver<LedgerEntry>) {
        let (tx, rx) = mpsc::channel(config.buffer_size);
        (
            Self {
                tx,
                drop_oldest_receiver: None,
                config,
            },
            rx,
        )
    }

    /// Variant of [`Self::new`] that wraps the receiver in
    /// `Arc<Mutex<>>`. Required when [`OverflowPolicy::DropOldest`] is
    /// in effect so `forward` can pop the oldest entry before
    /// re-sending. Pair with [`Self::run_with_poster_shared`].
    pub fn new_shared(
        config: ForwarderConfig,
    ) -> (Self, Arc<Mutex<mpsc::Receiver<LedgerEntry>>>) {
        let (tx, rx) = mpsc::channel(config.buffer_size);
        let rx = Arc::new(Mutex::new(rx));
        (
            Self {
                tx,
                drop_oldest_receiver: Some(Arc::clone(&rx)),
                config,
            },
            rx,
        )
    }

    /// Config accessor.
    pub fn config(&self) -> &ForwarderConfig {
        &self.config
    }

    /// Submit an entry to the forwarder according to the configured
    /// [`OverflowPolicy`].
    pub async fn forward(&self, entry: LedgerEntry) -> Result<(), ForwardError> {
        match self.config.overflow_policy {
            OverflowPolicy::DropNewest => match self.tx.try_send(entry) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => Err(ForwardError::BufferFull),
                Err(mpsc::error::TrySendError::Closed(_)) => Err(ForwardError::ChannelClosed),
            },
            OverflowPolicy::Block => self
                .tx
                .send(entry)
                .await
                .map_err(|_| ForwardError::ChannelClosed),
            OverflowPolicy::DropOldest => match self.tx.try_send(entry) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(entry)) => {
                    if let Some(rx) = &self.drop_oldest_receiver {
                        let mut guard = rx.lock().await;
                        let _ = guard.try_recv(); // discard oldest
                    }
                    self.tx.try_send(entry).map_err(|e| match e {
                        mpsc::error::TrySendError::Full(_) => ForwardError::BufferFull,
                        mpsc::error::TrySendError::Closed(_) => ForwardError::ChannelClosed,
                    })
                }
                Err(mpsc::error::TrySendError::Closed(_)) => Err(ForwardError::ChannelClosed),
            },
        }
    }

    /// Run the forwarder with the default [`StubPoster`].
    pub async fn run(receiver: mpsc::Receiver<LedgerEntry>, config: ForwarderConfig) {
        Self::run_with_poster(receiver, config, Arc::new(StubPoster::new())).await;
    }

    /// Run the batcher with a user-supplied transport. Returns once the
    /// matching sender has been dropped.
    pub async fn run_with_poster(
        mut receiver: mpsc::Receiver<LedgerEntry>,
        config: ForwarderConfig,
        poster: Arc<dyn BatchPoster>,
    ) {
        let mut batch: Vec<LedgerEntry> = Vec::with_capacity(config.batch_size);
        let flush_interval = Duration::from_millis(config.flush_interval_ms);

        loop {
            let recv = tokio::time::timeout(flush_interval, receiver.recv()).await;
            match recv {
                Ok(Some(entry)) => {
                    batch.push(entry);
                    if batch.len() >= config.batch_size {
                        Self::dispatch_batch(&poster, &config, &mut batch).await;
                    }
                }
                Ok(None) => {
                    if !batch.is_empty() {
                        Self::dispatch_batch(&poster, &config, &mut batch).await;
                    }
                    break;
                }
                Err(_) => {
                    if !batch.is_empty() {
                        Self::dispatch_batch(&poster, &config, &mut batch).await;
                    }
                }
            }
        }
    }

    /// Variant of [`Self::run_with_poster`] that accepts the shared
    /// `Arc<Mutex<Receiver>>` produced by [`Self::new_shared`].
    pub async fn run_with_poster_shared(
        receiver: Arc<Mutex<mpsc::Receiver<LedgerEntry>>>,
        config: ForwarderConfig,
        poster: Arc<dyn BatchPoster>,
    ) {
        let mut batch: Vec<LedgerEntry> = Vec::with_capacity(config.batch_size);
        let flush_interval = Duration::from_millis(config.flush_interval_ms);

        loop {
            let recv_result = {
                let mut rx = receiver.lock().await;
                tokio::time::timeout(flush_interval, rx.recv()).await
            };
            match recv_result {
                Ok(Some(entry)) => {
                    batch.push(entry);
                    if batch.len() >= config.batch_size {
                        Self::dispatch_batch(&poster, &config, &mut batch).await;
                    }
                }
                Ok(None) => {
                    if !batch.is_empty() {
                        Self::dispatch_batch(&poster, &config, &mut batch).await;
                    }
                    break;
                }
                Err(_) => {
                    if !batch.is_empty() {
                        Self::dispatch_batch(&poster, &config, &mut batch).await;
                    }
                }
            }
        }
    }

    /// Send `batch` with exponential backoff. Leaves `batch` empty so
    /// the caller can reuse the allocation.
    async fn dispatch_batch(
        poster: &Arc<dyn BatchPoster>,
        config: &ForwarderConfig,
        batch: &mut Vec<LedgerEntry>,
    ) {
        let to_send = std::mem::take(batch);
        let mut attempt: u32 = 0;
        loop {
            match poster.post(&config.endpoint, &to_send).await {
                Ok(()) => return,
                Err(e) => {
                    if attempt >= config.max_retries {
                        tracing::warn!(
                            endpoint = %config.endpoint,
                            batch_size = to_send.len(),
                            attempts = attempt + 1,
                            error = %e,
                            "log forwarding batch dropped after max_retries"
                        );
                        return;
                    }
                    let delay = backoff_delay(config.base_delay_ms, attempt);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }
}

/// Compute exponential backoff with deterministic jitter derived from
/// the attempt counter. Exposed so tests can assert monotonic growth.
pub fn backoff_delay(base_ms: u64, attempt: u32) -> Duration {
    let shift = attempt.min(16);
    let exp = base_ms.saturating_mul(1u64 << shift);
    // Deterministic jitter (0..base_ms/4) so tests remain reproducible.
    let jitter_cap = (base_ms / 4).max(1);
    let jitter = u64::from(attempt).saturating_mul(7) % jitter_cap;
    Duration::from_millis(exp.saturating_add(jitter))
}

// ---------------------------------------------------------------------------
// Test fixtures (exposed to integration tests)
// ---------------------------------------------------------------------------

/// Build a minimal [`LedgerEntry`] for tests.
#[doc(hidden)]
pub fn ledger_entry_fixture() -> LedgerEntry {
    LedgerEntry::new(
        Utc::now(),
        "tester",
        "host",
        serde_json::json!({"SessionStarted": {}}),
        None,
    )
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{Duration, sleep};

    fn cfg() -> ForwarderConfig {
        ForwarderConfig {
            endpoint: "stub://test".into(),
            buffer_size: 8,
            batch_size: 4,
            flush_interval_ms: 50,
            overflow_policy: OverflowPolicy::DropNewest,
            max_retries: 0,
            base_delay_ms: 10,
        }
    }

    // rtmx:req REQ-AUDIT-018
    #[tokio::test]
    async fn test_forwarder_accepts_entries_via_channel() {
        let (forwarder, mut rx) = LogForwarder::new(cfg());

        let entry = ledger_entry_fixture();
        forwarder.forward(entry.clone()).await.unwrap();

        let received = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("recv should not time out")
            .expect("channel should yield the entry");

        assert_eq!(received.os_user, entry.os_user);
        assert_eq!(received.hostname, entry.hostname);
    }

    // rtmx:req REQ-AUDIT-018
    #[tokio::test]
    async fn test_forwarder_batches_entries() {
        let mut c = cfg();
        c.buffer_size = 32;
        c.batch_size = 5;
        c.flush_interval_ms = 10_000; // size-driven, not time-driven
        c.max_retries = 0;

        let (forwarder, rx) = LogForwarder::new(c.clone());
        let poster = Arc::new(StubPoster::new());
        let poster_dyn: Arc<dyn BatchPoster> = Arc::clone(&poster) as _;
        let join = tokio::spawn(LogForwarder::run_with_poster(rx, c.clone(), poster_dyn));

        for _ in 0..12 {
            forwarder.forward(ledger_entry_fixture()).await.unwrap();
        }

        drop(forwarder);
        join.await.unwrap();

        let batches = poster.batches().await;
        let total: usize = batches.iter().map(Vec::len).sum();
        assert_eq!(total, 12, "all entries should be delivered");
        assert!(
            batches.iter().filter(|b| b.len() == 5).count() >= 2,
            "expected at least two batches of size 5, got {batches:?}"
        );
    }

    // rtmx:req REQ-AUDIT-019
    #[tokio::test]
    async fn test_buffer_overflow_drops_oldest_or_blocks() {
        // DropNewest branch: third send should fail fast.
        let mut c = cfg();
        c.buffer_size = 2;
        c.overflow_policy = OverflowPolicy::DropNewest;
        let (forwarder, _rx) = LogForwarder::new(c);
        forwarder.forward(ledger_entry_fixture()).await.unwrap();
        forwarder.forward(ledger_entry_fixture()).await.unwrap();
        match forwarder.forward(ledger_entry_fixture()).await {
            Err(ForwardError::BufferFull) => {}
            other => panic!("expected BufferFull, got {other:?}"),
        }
    }

    // rtmx:req REQ-AUDIT-019
    #[tokio::test]
    async fn test_buffer_overflow_drop_newest() {
        let mut c = cfg();
        c.buffer_size = 1;
        c.overflow_policy = OverflowPolicy::DropNewest;
        let (forwarder, _rx) = LogForwarder::new(c);
        forwarder.forward(ledger_entry_fixture()).await.unwrap();
        assert!(matches!(
            forwarder.forward(ledger_entry_fixture()).await,
            Err(ForwardError::BufferFull)
        ));
    }

    // rtmx:req REQ-AUDIT-019
    #[tokio::test]
    async fn test_buffer_overflow_block() {
        let mut c = cfg();
        c.buffer_size = 1;
        c.overflow_policy = OverflowPolicy::Block;
        let (forwarder, mut rx) = LogForwarder::new(c);

        forwarder.forward(ledger_entry_fixture()).await.unwrap();

        // Second forward blocks until we drain. Drive concurrently.
        tokio::join!(
            async {
                sleep(Duration::from_millis(50)).await;
                let _ = rx.recv().await;
            },
            async {
                forwarder.forward(ledger_entry_fixture()).await.unwrap();
            }
        );
    }

    // rtmx:req REQ-AUDIT-019
    #[tokio::test]
    async fn test_buffer_overflow_drop_oldest() {
        let mut c = cfg();
        c.buffer_size = 2;
        c.overflow_policy = OverflowPolicy::DropOldest;
        let (forwarder, rx) = LogForwarder::new_shared(c);

        let mut a = ledger_entry_fixture();
        a.os_user = "oldest".into();
        let mut b = ledger_entry_fixture();
        b.os_user = "middle".into();
        let mut third = ledger_entry_fixture();
        third.os_user = "newest".into();

        forwarder.forward(a).await.unwrap();
        forwarder.forward(b).await.unwrap();
        forwarder.forward(third).await.unwrap();

        // The oldest ("oldest") should have been dropped; "middle" and
        // "newest" should remain in the channel.
        let mut guard = rx.lock().await;
        let first = guard.recv().await.unwrap();
        let second = guard.recv().await.unwrap();
        assert_eq!(first.os_user, "middle");
        assert_eq!(second.os_user, "newest");
    }

    // A `BatchPoster` that fails `fail_remaining` times, then succeeds.
    struct FlakyPoster {
        fail_remaining: AtomicUsize,
        attempts: AtomicUsize,
    }

    #[async_trait]
    impl BatchPoster for FlakyPoster {
        async fn post(&self, _endpoint: &str, _batch: &[LedgerEntry]) -> Result<(), PostError> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail_remaining.load(Ordering::SeqCst) > 0 {
                self.fail_remaining.fetch_sub(1, Ordering::SeqCst);
                Err(PostError::new("simulated transient failure"))
            } else {
                Ok(())
            }
        }
    }

    // rtmx:req REQ-AUDIT-020
    #[tokio::test]
    async fn test_run_with_poster_retries_until_success() {
        let mut c = cfg();
        c.batch_size = 1;
        c.flush_interval_ms = 20;
        c.max_retries = 3;
        c.base_delay_ms = 1;

        let (forwarder, rx) = LogForwarder::new(c.clone());
        let poster = Arc::new(FlakyPoster {
            fail_remaining: AtomicUsize::new(2),
            attempts: AtomicUsize::new(0),
        });
        let poster_dyn: Arc<dyn BatchPoster> = Arc::clone(&poster) as _;

        let join = tokio::spawn(LogForwarder::run_with_poster(rx, c, poster_dyn));

        forwarder.forward(ledger_entry_fixture()).await.unwrap();
        drop(forwarder);
        join.await.unwrap();

        assert_eq!(
            poster.attempts.load(Ordering::SeqCst),
            3,
            "should have retried twice before succeeding"
        );
    }

    // rtmx:req REQ-AUDIT-020
    #[test]
    fn test_backoff_delay_increases() {
        let d0 = backoff_delay(100, 0);
        let d1 = backoff_delay(100, 1);
        let d2 = backoff_delay(100, 2);
        let d3 = backoff_delay(100, 3);
        assert!(d1 > d0, "delay should grow: d0={d0:?} d1={d1:?}");
        assert!(d2 > d1, "delay should grow: d1={d1:?} d2={d2:?}");
        assert!(d3 > d2, "delay should grow: d2={d2:?} d3={d3:?}");
    }
}
