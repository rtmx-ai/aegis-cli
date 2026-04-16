//! Integration tests for REQ-AUDIT-020: log forwarding retry with
//! exponential backoff.
//!
//! These tests drive the retry loop through a scripted in-memory
//! [`BatchPoster`] implementation. We deliberately avoid spinning up a
//! real HTTP server here: aegis-audit does not (and must not) depend
//! on `reqwest` or `wiremock`, and the retry logic under test is
//! transport-agnostic by design. The transport abstraction is what
//! this requirement actually guarantees -- a later requirement
//! (REQ-AUDIT-011 / REQ-AUDIT-012) plugs in concrete syslog / HTTPS
//! transports and will cover wire-level behaviour separately.

use aegis_audit::forwarding::{
    BatchPoster, ForwarderConfig, LedgerEntry, LogForwarder, OverflowPolicy, PostError,
    backoff_delay, ledger_entry_fixture,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Scripted transport returning a queue of results. Each `post()`
/// consumes the next scripted outcome; once the script is exhausted
/// all subsequent calls succeed.
struct ScriptedPoster {
    script: Mutex<Vec<Result<(), PostError>>>,
    attempts: AtomicUsize,
    attempt_times: Mutex<Vec<Instant>>,
    delivered: Mutex<Vec<Vec<LedgerEntry>>>,
}

impl ScriptedPoster {
    fn new(script: Vec<Result<(), PostError>>) -> Self {
        Self {
            script: Mutex::new(script),
            attempts: AtomicUsize::new(0),
            attempt_times: Mutex::new(Vec::new()),
            delivered: Mutex::new(Vec::new()),
        }
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    async fn delivered_batches(&self) -> Vec<Vec<LedgerEntry>> {
        self.delivered.lock().await.clone()
    }

    async fn attempt_times(&self) -> Vec<Instant> {
        self.attempt_times.lock().await.clone()
    }
}

#[async_trait]
impl BatchPoster for ScriptedPoster {
    async fn post(&self, _endpoint: &str, batch: &[LedgerEntry]) -> Result<(), PostError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.attempt_times.lock().await.push(Instant::now());

        let outcome = {
            let mut script = self.script.lock().await;
            if script.is_empty() {
                Ok(())
            } else {
                script.remove(0)
            }
        };

        if outcome.is_ok() {
            self.delivered.lock().await.push(batch.to_vec());
        }
        outcome
    }
}

fn test_config() -> ForwarderConfig {
    ForwarderConfig {
        endpoint: "siem://test".into(),
        buffer_size: 16,
        batch_size: 1, // one entry per batch so retry paths are deterministic
        flush_interval_ms: 20,
        overflow_policy: OverflowPolicy::DropNewest,
        max_retries: 3,
        base_delay_ms: 5,
    }
}

// rtmx:req REQ-AUDIT-020
#[tokio::test]
async fn test_forwarding_retries_with_backoff() {
    // Two transient failures (503-equivalent), then success.
    let poster = Arc::new(ScriptedPoster::new(vec![
        Err(PostError::new("503 Service Unavailable")),
        Err(PostError::new("503 Service Unavailable")),
        Ok(()),
    ]));
    let poster_dyn: Arc<dyn BatchPoster> = Arc::clone(&poster) as _;

    let config = test_config();
    let (forwarder, rx) = LogForwarder::new(config.clone());
    let join = tokio::spawn(LogForwarder::run_with_poster(rx, config, poster_dyn));

    forwarder.forward(ledger_entry_fixture()).await.unwrap();
    drop(forwarder);
    join.await.unwrap();

    assert_eq!(
        poster.attempts(),
        3,
        "should have retried twice before succeeding"
    );
    let delivered = poster.delivered_batches().await;
    assert_eq!(
        delivered.len(),
        1,
        "exactly one batch should have been delivered"
    );
    assert_eq!(
        delivered[0].len(),
        1,
        "the delivered batch should contain the single entry"
    );
}

// rtmx:req REQ-AUDIT-020
#[tokio::test]
async fn test_forwarding_gives_up_after_max_retries() {
    // max_retries=3 => 4 total attempts (initial + 3 retries).
    let poster = Arc::new(ScriptedPoster::new(vec![
        Err(PostError::new("503")),
        Err(PostError::new("503")),
        Err(PostError::new("503")),
        Err(PostError::new("503")),
        Err(PostError::new("503")),
    ]));
    let poster_dyn: Arc<dyn BatchPoster> = Arc::clone(&poster) as _;

    let config = test_config();
    let (forwarder, rx) = LogForwarder::new(config.clone());
    let join = tokio::spawn(LogForwarder::run_with_poster(rx, config, poster_dyn));

    forwarder.forward(ledger_entry_fixture()).await.unwrap();
    drop(forwarder);
    join.await.unwrap();

    assert_eq!(
        poster.attempts(),
        4,
        "initial attempt + max_retries=3 => 4 total attempts"
    );
    assert!(
        poster.delivered_batches().await.is_empty(),
        "no batch should be delivered when all attempts fail"
    );
}

// rtmx:req REQ-AUDIT-020
#[tokio::test]
async fn test_backoff_delay_increases() {
    // Pure function: delays strictly increase with attempt number.
    let d0 = backoff_delay(100, 0);
    let d1 = backoff_delay(100, 1);
    let d2 = backoff_delay(100, 2);
    let d3 = backoff_delay(100, 3);

    assert!(
        d1 > d0,
        "attempt 1 ({d1:?}) should exceed attempt 0 ({d0:?})"
    );
    assert!(
        d2 > d1,
        "attempt 2 ({d2:?}) should exceed attempt 1 ({d1:?})"
    );
    assert!(
        d3 > d2,
        "attempt 3 ({d3:?}) should exceed attempt 2 ({d2:?})"
    );

    // Roughly doubles (allowing for jitter <= base/4).
    let ratio_1_to_0 = d1.as_millis() as f64 / d0.as_millis().max(1) as f64;
    assert!(
        (1.5..=3.0).contains(&ratio_1_to_0),
        "exponential growth expected, got ratio {ratio_1_to_0}"
    );
}

// rtmx:req REQ-AUDIT-020
#[tokio::test]
async fn test_backoff_delay_observed_in_retry_loop() {
    // Verify that between-attempt delays actually grow at runtime.
    let poster = Arc::new(ScriptedPoster::new(vec![
        Err(PostError::new("503")),
        Err(PostError::new("503")),
        Err(PostError::new("503")),
        Ok(()),
    ]));
    let poster_dyn: Arc<dyn BatchPoster> = Arc::clone(&poster) as _;

    let mut config = test_config();
    config.base_delay_ms = 20;
    let (forwarder, rx) = LogForwarder::new(config.clone());
    let join = tokio::spawn(LogForwarder::run_with_poster(rx, config, poster_dyn));

    forwarder.forward(ledger_entry_fixture()).await.unwrap();
    drop(forwarder);
    join.await.unwrap();

    let times = poster.attempt_times().await;
    assert_eq!(times.len(), 4, "expected 4 attempts, got {}", times.len());

    let gap_0_1 = times[1].duration_since(times[0]);
    let gap_1_2 = times[2].duration_since(times[1]);
    let gap_2_3 = times[3].duration_since(times[2]);

    // Allow generous lower bounds (wall clock, scheduler jitter).
    assert!(
        gap_0_1 >= Duration::from_millis(15),
        "first backoff should be >= ~base_delay, got {gap_0_1:?}"
    );
    assert!(
        gap_1_2 > gap_0_1 / 2,
        "second backoff ({gap_1_2:?}) should be meaningfully larger than first ({gap_0_1:?})"
    );
    assert!(
        gap_2_3 > gap_1_2 / 2,
        "third backoff ({gap_2_3:?}) should be meaningfully larger than second ({gap_1_2:?})"
    );
}
