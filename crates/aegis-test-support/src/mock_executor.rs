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
}

impl Default for MockToolExecutor {
    fn default() -> Self {
        Self {
            results: Mutex::new(HashMap::new()),
            default_result: ToolResult::Success {
                output: "mock output".to_string(),
            },
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
        let name = Self::tool_name(tool_call);
        let results = self.results.lock().unwrap();
        Ok(results
            .get(&name)
            .cloned()
            .unwrap_or_else(|| self.default_result.clone()))
    }
}
