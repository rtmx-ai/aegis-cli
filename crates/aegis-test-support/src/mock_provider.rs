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
use std::sync::{Arc, Mutex};

struct MockLlmInner {
    responses: Mutex<VecDeque<Vec<StreamEvent>>>,
    /// Tool schemas received on each `stream()` call, for test assertions.
    captured_tool_schemas: Mutex<Vec<Vec<ToolSchema>>>,
    /// Messages received on each `stream()` call, for test assertions.
    captured_messages: Mutex<Vec<Vec<Message>>>,
}

/// A mock provider that returns pre-configured responses in sequence.
///
/// Cloning is cheap (shared `Arc` state) so the provider can be
/// inspected after being moved into `AgentLoop::new()`.
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_test_support::mock_provider::MockLlmProvider;
///
/// let provider = MockLlmProvider::new();
/// // Queue responses before passing to the agent loop.
/// ```
#[derive(Clone)]
pub struct MockLlmProvider {
    inner: Arc<MockLlmInner>,
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self {
            inner: Arc::new(MockLlmInner {
                responses: Mutex::new(VecDeque::new()),
                captured_tool_schemas: Mutex::new(Vec::new()),
                captured_messages: Mutex::new(Vec::new()),
            }),
        }
    }
}

impl MockLlmProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a sequence of stream events to be returned on the next call.
    pub fn queue_response(&self, events: Vec<StreamEvent>) {
        self.inner.responses.lock().unwrap().push_back(events);
    }

    /// Tool schemas captured from each `stream()` call.
    pub fn captured_tool_schemas(&self) -> Vec<Vec<ToolSchema>> {
        self.inner.captured_tool_schemas.lock().unwrap().clone()
    }

    /// Messages captured from each `stream()` call.
    pub fn captured_messages(&self) -> Vec<Vec<Message>> {
        self.inner.captured_messages.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        self.inner
            .captured_messages
            .lock()
            .unwrap()
            .push(messages.to_vec());
        self.inner
            .captured_tool_schemas
            .lock()
            .unwrap()
            .push(tools.to_vec());
        let events = self
            .inner
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| DomainError::ProviderError {
                message: "MockLlmProvider: no more queued responses".into(),
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
