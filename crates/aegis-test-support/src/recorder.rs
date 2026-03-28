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

    // @req REQ-TEST-002
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

    // @req REQ-TEST-002
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

    // @req REQ-TEST-002
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

    // @req REQ-TEST-002
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
}
