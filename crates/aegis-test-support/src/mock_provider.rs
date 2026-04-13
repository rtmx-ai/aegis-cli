//! Mock LLM provider for deterministic testing.
//!
//! Supports two modes:
//! - Canned responses: Return pre-configured responses for any input.
//! - Record/Replay: Record real LLM responses to JSON, replay in CI.
//!   (Follows Goose's TestProvider pattern.)

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;
use std::collections::VecDeque;

/// A mock provider that returns pre-configured responses in sequence.
pub struct MockLlmProvider {
    responses: std::sync::Mutex<VecDeque<Vec<StreamEvent>>>,
    /// Tool schemas received on each `stream()` call, for test assertions.
    pub captured_tool_schemas: std::sync::Mutex<Vec<Vec<ToolSchema>>>,
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self {
            responses: std::sync::Mutex::new(VecDeque::new()),
            captured_tool_schemas: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl MockLlmProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a sequence of stream events to be returned on the next call.
    pub fn queue_response(&self, events: Vec<StreamEvent>) {
        self.responses.lock().unwrap().push_back(events);
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn stream(
        &self,
        _messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        self.captured_tool_schemas
            .lock()
            .unwrap()
            .push(tools.to_vec());
        let events = self.responses.lock().unwrap().pop_front().ok_or_else(|| {
            DomainError::ProviderError {
                message: "MockLlmProvider: no more queued responses".into(),
            }
        })?;
        Ok(Box::new(MockTokenStream { events }))
    }
}

struct MockTokenStream {
    events: Vec<StreamEvent>,
}

#[async_trait]
impl TokenStream for MockTokenStream {
    async fn next(&mut self) -> Option<StreamEvent> {
        if self.events.is_empty() {
            None
        } else {
            Some(self.events.remove(0))
        }
    }
}
