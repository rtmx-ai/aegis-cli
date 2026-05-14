//! Mock tool executor for testing.

use aegis_domain::error::DomainError;
use aegis_domain::ports::ToolExecutor;
use aegis_domain::types::{ToolCall, ToolResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

/// A mock executor that returns canned results per tool name.
pub struct MockToolExecutor {
    results: Mutex<HashMap<String, ToolResult>>,
    default_result: ToolResult,
    /// Records every tool call executed (for assertion in tests).
    recorded_calls: Mutex<Vec<ToolCall>>,
}

impl Default for MockToolExecutor {
    fn default() -> Self {
        Self {
            results: Mutex::new(HashMap::new()),
            default_result: ToolResult::Success {
                output: "mock output".to_string(),
            },
            recorded_calls: Mutex::new(Vec::new()),
        }
    }
}

impl MockToolExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a canned result for a specific tool call type.
    pub fn set_result(&self, tool_name: &str, result: ToolResult) {
        self.results
            .lock()
            .unwrap()
            .insert(tool_name.to_string(), result);
    }

    /// Returns a snapshot of all tool calls that were executed.
    pub fn recorded_calls(&self) -> Vec<ToolCall> {
        self.recorded_calls.lock().unwrap().clone()
    }

    fn tool_name(call: &ToolCall) -> String {
        match call {
            ToolCall::ReadFile { .. } => "read_file".to_string(),
            ToolCall::WriteFile { .. } => "write_file".to_string(),
            ToolCall::RunCommand { .. } => "run_command".to_string(),
            ToolCall::ListDir { .. } => "list_dir".to_string(),
            ToolCall::Grep { .. } => "grep".to_string(),
            ToolCall::McpTool { qualified_name, .. } => qualified_name.clone(),
        }
    }
}

#[async_trait]
impl ToolExecutor for MockToolExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult, DomainError> {
        self.recorded_calls.lock().unwrap().push(tool_call.clone());
        let name = Self::tool_name(tool_call);
        let results = self.results.lock().unwrap();
        Ok(results
            .get(&name)
            .cloned()
            .unwrap_or_else(|| self.default_result.clone()))
    }
}

/// Shared mock executor that can be inspected after the agent loop consumes it.
pub struct SharedMockExecutor(pub std::sync::Arc<MockToolExecutor>);

impl SharedMockExecutor {
    pub fn new() -> (Self, std::sync::Arc<MockToolExecutor>) {
        let inner = std::sync::Arc::new(MockToolExecutor::new());
        (Self(std::sync::Arc::clone(&inner)), inner)
    }
}

#[async_trait]
impl ToolExecutor for SharedMockExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult, DomainError> {
        self.0.execute(tool_call).await
    }
}
