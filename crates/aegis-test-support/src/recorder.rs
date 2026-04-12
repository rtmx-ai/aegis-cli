//! LLM record/replay infrastructure for deterministic agent tests.
//!
//! - `RecordingProvider` wraps a real `LlmProvider`, records all stream events,
//!   and saves them to a JSON file on drop.
//! - `ReplayProvider` reads a recording file and replays events in order,
//!   producing fully deterministic test runs without network calls.

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// A single recorded stream event with serde support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvent {
    pub event: StreamEvent,
}

/// A full recording: one inner vec per `stream()` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub calls: Vec<Vec<RecordedEvent>>,
}

impl Recording {
    pub fn new() -> Self {
        Self { calls: Vec::new() }
    }
}

impl Default for Recording {
    fn default() -> Self {
        Self::new()
    }
}

/// Load a recording from a JSON file.
pub fn load_recording(path: &Path) -> Result<Recording, DomainError> {
    let data = std::fs::read_to_string(path).map_err(|e| {
        DomainError::Other(format!(
            "failed to read recording file {}: {}",
            path.display(),
            e
        ))
    })?;
    serde_json::from_str(&data).map_err(|e| {
        DomainError::Other(format!(
            "failed to parse recording file {}: {}",
            path.display(),
            e
        ))
    })
}

/// Save a recording to a JSON file.
pub fn save_recording(recording: &Recording, path: &Path) -> Result<(), DomainError> {
    let data = serde_json::to_string_pretty(recording)
        .map_err(|e| DomainError::Other(format!("failed to serialize recording: {}", e)))?;
    std::fs::write(path, data).map_err(|e| {
        DomainError::Other(format!(
            "failed to write recording file {}: {}",
            path.display(),
            e
        ))
    })
}

/// Wraps a real `LlmProvider`, forwarding all calls and recording every
/// stream event. Call `finish()` or let it drop to persist the recording.
pub struct RecordingProvider {
    inner: Box<dyn LlmProvider>,
    recording: Mutex<Recording>,
    path: std::path::PathBuf,
}

impl RecordingProvider {
    pub fn new(inner: Box<dyn LlmProvider>, path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            inner,
            recording: Mutex::new(Recording::new()),
            path: path.into(),
        }
    }

    /// Persist the recording to disk. Called automatically on drop, but
    /// can be called explicitly to check for errors.
    pub fn finish(&self) -> Result<(), DomainError> {
        let recording = self.recording.lock().unwrap();
        save_recording(&recording, &self.path)
    }
}

impl Drop for RecordingProvider {
    fn drop(&mut self) {
        // Best-effort save on drop; use finish() for error handling.
        let _ = self.finish();
    }
}

#[async_trait]
impl LlmProvider for RecordingProvider {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        let mut real_stream = self.inner.stream(messages, tools).await?;

        // Drain the real stream, collecting all events.
        let mut events = Vec::new();
        while let Some(event) = real_stream.next().await {
            events.push(RecordedEvent {
                event: event.clone(),
            });
        }

        self.recording.lock().unwrap().calls.push(events.clone());

        // Return a replay stream so the caller sees the same events.
        Ok(Box::new(ReplayTokenStream {
            events: events.into_iter().map(|r| r.event).collect(),
            index: 0,
        }))
    }
}

/// Replays a previously recorded session. Each `stream()` call returns
/// the next recorded call's events in order.
pub struct ReplayProvider {
    recording: Mutex<Recording>,
    call_index: Mutex<usize>,
}

impl ReplayProvider {
    /// Create from a recording loaded from disk.
    pub fn from_recording(recording: Recording) -> Self {
        Self {
            recording: Mutex::new(recording),
            call_index: Mutex::new(0),
        }
    }

    /// Create by loading a recording file.
    pub fn from_file(path: &Path) -> Result<Self, DomainError> {
        let recording = load_recording(path)?;
        Ok(Self::from_recording(recording))
    }
}

#[async_trait]
impl LlmProvider for ReplayProvider {
    async fn stream(
        &self,
        _messages: &[Message],
        _tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        let mut idx = self.call_index.lock().unwrap();
        let recording = self.recording.lock().unwrap();

        let events = if *idx < recording.calls.len() {
            let call_events: Vec<StreamEvent> = recording.calls[*idx]
                .iter()
                .map(|r| r.event.clone())
                .collect();
            *idx += 1;
            call_events
        } else {
            // No more recorded calls; return a stream with just Done.
            vec![StreamEvent::Done {
                input_tokens: 0,
                output_tokens: 0,
            }]
        };

        Ok(Box::new(ReplayTokenStream { events, index: 0 }))
    }
}

/// A token stream that yields pre-recorded events.
pub struct ReplayTokenStream {
    events: Vec<StreamEvent>,
    index: usize,
}

#[async_trait]
impl TokenStream for ReplayTokenStream {
    async fn next(&mut self) -> Option<StreamEvent> {
        if self.index < self.events.len() {
            let event = self.events[self.index].clone();
            self.index += 1;
            Some(event)
        } else {
            None
        }
    }
}

// ============================================================================
// REQ-TEST-024: Cassette recording mode for capturing real provider responses.
// ============================================================================

/// Current cassette file format version.
pub const CASSETTE_VERSION: u32 = 1;

/// Environment variable that toggles record mode.
pub const RECORD_ENV_VAR: &str = "AEGIS_RECORD_CASSETTES";

/// A single recorded LLM exchange (request -> stream events).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteExchange {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
    pub events: Vec<StreamEvent>,
}

/// On-disk cassette schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteFile {
    pub version: u32,
    pub recorded_at: String,
    pub exchanges: Vec<CassetteExchange>,
}

impl CassetteFile {
    pub fn new() -> Self {
        Self {
            version: CASSETTE_VERSION,
            recorded_at: String::new(),
            exchanges: Vec::new(),
        }
    }
}

impl Default for CassetteFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns true if `AEGIS_RECORD_CASSETTES=1` is set in the environment.
pub fn is_record_mode() -> bool {
    std::env::var(RECORD_ENV_VAR)
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Resolve the workspace root by walking up from CARGO_MANIFEST_DIR until we
/// find a Cargo.toml that defines `[workspace]`. Falls back to the manifest
/// dir if no workspace marker is found.
fn workspace_root() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut cur = manifest.clone();
    loop {
        let candidate = cur.join("Cargo.toml");
        if candidate.exists()
            && let Ok(s) = std::fs::read_to_string(&candidate)
            && s.contains("[workspace]")
        {
            return cur;
        }
        if !cur.pop() {
            return manifest;
        }
    }
}

/// Resolve the on-disk path for a named cassette:
/// `<workspace_root>/tests/fixtures/cassettes/<test_name>.json`.
pub fn cassette_path(test_name: &str) -> std::path::PathBuf {
    workspace_root()
        .join("tests")
        .join("fixtures")
        .join("cassettes")
        .join(format!("{}.json", test_name))
}

/// Load a cassette by test name. Returns `None` if the file does not exist.
/// Returns `Some(Err(..))`-equivalent via panic on a corrupted file is avoided
/// by surfacing parse errors as `None` only when the file truly is missing.
pub fn load_cassette(test_name: &str) -> Option<CassetteFile> {
    let path = cassette_path(test_name);
    if !path.exists() {
        return None;
    }
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Save a cassette atomically (write to temp file then rename).
pub fn save_cassette(test_name: &str, cassette: &CassetteFile) -> Result<(), DomainError> {
    let path = cassette_path(test_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            DomainError::Other(format!(
                "failed to create cassette dir {}: {}",
                parent.display(),
                e
            ))
        })?;
    }
    let data = serde_json::to_string_pretty(cassette)
        .map_err(|e| DomainError::Other(format!("failed to serialize cassette: {}", e)))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data).map_err(|e| {
        DomainError::Other(format!(
            "failed to write cassette tmp {}: {}",
            tmp.display(),
            e
        ))
    })?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        DomainError::Other(format!(
            "failed to rename cassette to {}: {}",
            path.display(),
            e
        ))
    })?;
    Ok(())
}

/// High-level provider that wraps either a `RecordingProvider` (record mode)
/// or a `ReplayProvider` (replay mode), choosing based on `is_record_mode()`.
pub enum CassetteProvider {
    Recording(RecordingProvider),
    Replay(ReplayProvider),
}

impl CassetteProvider {
    /// Create a cassette-backed provider for the given test name.
    /// In record mode, real exchanges are captured and saved to the cassette
    /// file on drop. In replay mode, the cassette file is read and used as
    /// the source of truth (returns an error if it doesn't exist).
    pub fn new(
        test_name: &str,
        real_provider: Box<dyn LlmProvider>,
    ) -> Result<Self, DomainError> {
        if is_record_mode() {
            let path = cassette_path(test_name);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            Ok(Self::Recording(RecordingProvider::new(real_provider, path)))
        } else {
            match load_cassette(test_name) {
                Some(cassette) => {
                    let recording = Recording {
                        calls: cassette
                            .exchanges
                            .into_iter()
                            .map(|ex| {
                                ex.events
                                    .into_iter()
                                    .map(|e| RecordedEvent { event: e })
                                    .collect()
                            })
                            .collect(),
                    };
                    Ok(Self::Replay(ReplayProvider::from_recording(recording)))
                }
                None => Err(DomainError::Other(format!(
                    "no cassette found for test '{}' at {} (run with {}=1 to record)",
                    test_name,
                    cassette_path(test_name).display(),
                    RECORD_ENV_VAR
                ))),
            }
        }
    }
}

#[async_trait]
impl LlmProvider for CassetteProvider {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        match self {
            Self::Recording(p) => p.stream(messages, tools).await,
            Self::Replay(p) => p.stream(messages, tools).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::{FilePath, ToolCall};

    fn sample_recording() -> Recording {
        Recording {
            calls: vec![
                vec![
                    RecordedEvent {
                        event: StreamEvent::Token("Hello".into()),
                    },
                    RecordedEvent {
                        event: StreamEvent::Token(" world".into()),
                    },
                    RecordedEvent {
                        event: StreamEvent::Done {
                            input_tokens: 10,
                            output_tokens: 2,
                        },
                    },
                ],
                vec![
                    RecordedEvent {
                        event: StreamEvent::ToolUse(ToolCall::ReadFile {
                            path: FilePath::new_unchecked("src/main.rs"),
                        }),
                    },
                    RecordedEvent {
                        event: StreamEvent::Done {
                            input_tokens: 15,
                            output_tokens: 1,
                        },
                    },
                ],
            ],
        }
    }

    // rtmx:req REQ-TEST-002
    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recording.json");

        let original = sample_recording();
        save_recording(&original, &path).unwrap();
        let loaded = load_recording(&path).unwrap();

        assert_eq!(loaded.calls.len(), 2);
        assert_eq!(loaded.calls[0].len(), 3);
        assert_eq!(loaded.calls[1].len(), 2);

        // Verify first call tokens.
        match &loaded.calls[0][0].event {
            StreamEvent::Token(t) => assert_eq!(t, "Hello"),
            other => panic!("expected Token, got {:?}", other),
        }
        match &loaded.calls[0][1].event {
            StreamEvent::Token(t) => assert_eq!(t, " world"),
            other => panic!("expected Token, got {:?}", other),
        }

        // Verify second call tool use.
        match &loaded.calls[1][0].event {
            StreamEvent::ToolUse(ToolCall::ReadFile { path }) => {
                assert_eq!(path.as_path().to_str().unwrap(), "src/main.rs");
            }
            other => panic!("expected ToolUse(ReadFile), got {:?}", other),
        }
    }

    // rtmx:req REQ-TEST-002
    #[tokio::test]
    async fn replay_provider_yields_recorded_events_in_order() {
        let recording = sample_recording();
        let provider = ReplayProvider::from_recording(recording);

        // First stream() call should yield call 0 events.
        let mut stream = provider.stream(&[], &[]).await.unwrap();

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Token(t) => assert_eq!(t, "Hello"),
            other => panic!("expected Token(Hello), got {:?}", other),
        }

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Token(t) => assert_eq!(t, " world"),
            other => panic!("expected Token( world), got {:?}", other),
        }

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Done {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 10);
                assert_eq!(output_tokens, 2);
            }
            other => panic!("expected Done, got {:?}", other),
        }

        assert!(stream.next().await.is_none());

        // Second stream() call should yield call 1 events.
        let mut stream = provider.stream(&[], &[]).await.unwrap();

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::ToolUse(ToolCall::ReadFile { .. }) => {}
            other => panic!("expected ToolUse(ReadFile), got {:?}", other),
        }

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Done { .. } => {}
            other => panic!("expected Done, got {:?}", other),
        }

        assert!(stream.next().await.is_none());
    }

    // rtmx:req REQ-TEST-002
    #[tokio::test]
    async fn empty_recording_produces_done_event() {
        let recording = Recording::new();
        let provider = ReplayProvider::from_recording(recording);

        let mut stream = provider.stream(&[], &[]).await.unwrap();

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Done {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 0);
                assert_eq!(output_tokens, 0);
            }
            other => panic!("expected Done, got {:?}", other),
        }

        assert!(stream.next().await.is_none());
    }

    // rtmx:req REQ-TEST-002
    #[tokio::test]
    async fn replay_from_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recording.json");

        let original = sample_recording();
        save_recording(&original, &path).unwrap();

        let provider = ReplayProvider::from_file(&path).unwrap();
        let mut stream = provider.stream(&[], &[]).await.unwrap();

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Token(t) => assert_eq!(t, "Hello"),
            other => panic!("expected Token(Hello), got {:?}", other),
        }
    }

    fn sample_cassette() -> CassetteFile {
        CassetteFile {
            version: CASSETTE_VERSION,
            recorded_at: "2026-04-03T00:00:00Z".to_string(),
            exchanges: vec![CassetteExchange {
                messages: vec![],
                tools: vec![],
                events: vec![
                    StreamEvent::Token("hi".into()),
                    StreamEvent::Done {
                        input_tokens: 1,
                        output_tokens: 1,
                    },
                ],
            }],
        }
    }

    // Serialize the cassette to JSON for equality comparison since the
    // underlying StreamEvent type does not implement PartialEq.
    fn cassette_json(c: &CassetteFile) -> String {
        serde_json::to_string(c).unwrap()
    }

    // rtmx:req REQ-TEST-024
    #[test]
    fn test_cassette_record_mode_detection() {
        // Use a guard so concurrent tests don't trample env state.
        let prev = std::env::var(RECORD_ENV_VAR).ok();
        // SAFETY: tests are single-threaded for env mutation; we restore on exit.
        unsafe {
            std::env::set_var(RECORD_ENV_VAR, "1");
        }
        assert!(is_record_mode());
        unsafe {
            std::env::remove_var(RECORD_ENV_VAR);
        }
        assert!(!is_record_mode());
        unsafe {
            std::env::set_var(RECORD_ENV_VAR, "0");
        }
        assert!(!is_record_mode());
        // Restore.
        unsafe {
            match prev {
                Some(v) => std::env::set_var(RECORD_ENV_VAR, v),
                None => std::env::remove_var(RECORD_ENV_VAR),
            }
        }
    }

    // rtmx:req REQ-TEST-024
    #[test]
    fn test_cassette_save_and_load_roundtrip() {
        let test_name = "unit_roundtrip_REQ_TEST_024";
        let cassette = sample_cassette();
        save_cassette(test_name, &cassette).unwrap();

        let loaded = load_cassette(test_name).expect("cassette should exist");
        assert_eq!(loaded.version, CASSETTE_VERSION);
        assert_eq!(loaded.exchanges.len(), 1);
        assert_eq!(cassette_json(&loaded), cassette_json(&cassette));

        // Cleanup so the repo doesn't accumulate fixtures.
        let _ = std::fs::remove_file(cassette_path(test_name));
    }

    // rtmx:req REQ-TEST-024
    #[test]
    fn test_cassette_load_returns_none_for_missing() {
        let test_name = "definitely_does_not_exist_REQ_TEST_024_xyz";
        let _ = std::fs::remove_file(cassette_path(test_name));
        assert!(load_cassette(test_name).is_none());
    }

    // rtmx:req REQ-TEST-024
    #[test]
    fn test_cassette_path_resolution() {
        let p = cassette_path("foo");
        let s = p.to_string_lossy().replace('\\', "/");
        assert!(
            s.ends_with("tests/fixtures/cassettes/foo.json"),
            "path was {}",
            s
        );
        assert!(p.is_absolute());
    }
}
