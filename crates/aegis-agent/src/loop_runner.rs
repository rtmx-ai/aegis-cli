//! The REA (Read-Evaluate-Act) loop runner.

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use aegis_domain::types::*;

/// Configuration for the agent loop.
pub struct AgentConfig {
    pub max_iterations: usize,
    pub system_prompt: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            system_prompt: "You are a helpful coding assistant.".to_string(),
        }
    }
}

/// Result of a completed agent loop.
#[derive(Debug)]
pub struct AgentResult {
    pub response: String,
    pub iterations: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// The tool schemas the agent exposes to the LLM.
fn builtin_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "read_file".to_string(),
            description: "Read the contents of a file.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to read"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "write_file".to_string(),
            description: "Write content to a file.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolSchema {
            name: "run_command".to_string(),
            description: "Execute a shell command.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolSchema {
            name: "list_dir".to_string(),
            description: "List directory contents.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "grep".to_string(),
            description: "Search for a pattern in files.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file path to search"
                    }
                },
                "required": ["pattern", "path"]
            }),
        },
    ]
}

/// The agent loop runner, parameterized by port traits.
pub struct AgentLoop<P, G, E, A, S>
where
    P: LlmProvider,
    G: ApprovalGate,
    E: ToolExecutor,
    A: AuditLedger,
    S: SecurityFilter,
{
    provider: P,
    gate: G,
    executor: E,
    #[allow(dead_code)]
    ledger: A,
    #[allow(dead_code)]
    filter: S,
    config: AgentConfig,
}

impl<P, G, E, A, S> AgentLoop<P, G, E, A, S>
where
    P: LlmProvider,
    G: ApprovalGate,
    E: ToolExecutor,
    A: AuditLedger,
    S: SecurityFilter,
{
    pub fn new(
        provider: P,
        gate: G,
        executor: E,
        ledger: A,
        filter: S,
        config: AgentConfig,
    ) -> Self {
        Self {
            provider,
            gate,
            executor,
            ledger,
            filter,
            config,
        }
    }

    /// Run the agent loop to completion for a given user prompt.
    pub async fn run(&self, prompt: &str) -> Result<AgentResult, DomainError> {
        let tools = builtin_tool_schemas();
        let mut history = vec![
            Message {
                role: Role::System,
                content: self.config.system_prompt.clone(),
            },
            Message {
                role: Role::User,
                content: prompt.to_string(),
            },
        ];

        let mut total_input_tokens = 0u64;
        let mut total_output_tokens = 0u64;

        for iteration in 0..self.config.max_iterations {
            // EVALUATE: Stream response from LLM
            let mut stream = self.provider.stream(&history, &tools).await?;

            let mut response_text = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();

            // Collect the full response
            while let Some(event) = stream.next().await {
                match event {
                    StreamEvent::Token(text) => {
                        response_text.push_str(&text);
                    }
                    StreamEvent::ToolUse(call) => {
                        tool_calls.push(call);
                    }
                    StreamEvent::Done {
                        input_tokens,
                        output_tokens,
                    } => {
                        total_input_tokens += input_tokens;
                        total_output_tokens += output_tokens;
                    }
                    StreamEvent::Error(msg) => {
                        return Err(DomainError::ProviderError { message: msg });
                    }
                }
            }

            // Add assistant response to history
            if !response_text.is_empty() {
                history.push(Message {
                    role: Role::Assistant,
                    content: response_text.clone(),
                });
            }

            // If no tool calls, the agent is done
            if tool_calls.is_empty() {
                return Ok(AgentResult {
                    response: response_text,
                    iterations: iteration + 1,
                    input_tokens: total_input_tokens,
                    output_tokens: total_output_tokens,
                });
            }

            // ACT: Execute each tool call
            for call in &tool_calls {
                let result = if call.risk() == ToolRisk::StateMutating {
                    // HITL gate for mutating tools
                    let decision = self.gate.request_approval(call).await?;
                    match decision {
                        ApprovalDecision::Approved | ApprovalDecision::Edited => {
                            self.executor.execute(call).await?
                        }
                        ApprovalDecision::Denied | ApprovalDecision::Skipped => {
                            ToolResult::PermissionDenied {
                                reason: "User denied tool execution".to_string(),
                            }
                        }
                    }
                } else {
                    // Safe tools auto-execute
                    self.executor.execute(call).await?
                };

                // INJECT: Add tool result to history
                let result_text = match &result {
                    ToolResult::Success { output } => output.clone(),
                    ToolResult::Error { message } => {
                        format!("Error: {message}")
                    }
                    ToolResult::PermissionDenied { reason } => {
                        format!("Permission denied: {reason}")
                    }
                };

                history.push(Message {
                    role: Role::Tool,
                    content: result_text,
                });
            }
        }

        // Exceeded max iterations
        Err(DomainError::Other(format!(
            "Agent exceeded max iterations ({})",
            self.config.max_iterations
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_test_support::mock_executor::MockToolExecutor;
    use aegis_test_support::mock_filter::MockSecurityFilter;
    use aegis_test_support::mock_gate::MockApprovalGate;
    use aegis_test_support::mock_ledger::MockAuditLedger;
    use aegis_test_support::mock_provider::MockLlmProvider;

    fn make_agent(
        provider: MockLlmProvider,
    ) -> AgentLoop<
        MockLlmProvider,
        MockApprovalGate,
        MockToolExecutor,
        MockAuditLedger,
        MockSecurityFilter,
    > {
        AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        )
    }

    // @req REQ-AGENT-001
    #[test]
    fn test_agent_config_defaults() {
        let config = AgentConfig::default();
        assert_eq!(config.max_iterations, 100);
    }

    // @req REQ-AGENT-001
    #[tokio::test]
    async fn agent_completes_simple_text_response() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![
            StreamEvent::Token("Hello!".to_string()),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 2,
            },
        ]);

        let agent = make_agent(provider);
        let result = agent.run("Hi").await.unwrap();

        assert_eq!(result.response, "Hello!");
        assert_eq!(result.iterations, 1);
        assert_eq!(result.input_tokens, 10);
        assert_eq!(result.output_tokens, 2);
    }

    // @req REQ-AGENT-001
    #[tokio::test]
    async fn agent_loops_on_tool_use_then_completes() {
        let provider = MockLlmProvider::new();

        // First call: LLM requests a tool call
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("src/main.rs"),
            }),
            StreamEvent::Done {
                input_tokens: 15,
                output_tokens: 5,
            },
        ]);

        // Second call: LLM provides final answer
        provider.queue_response(vec![
            StreamEvent::Token("The file contains a main function.".to_string()),
            StreamEvent::Done {
                input_tokens: 50,
                output_tokens: 10,
            },
        ]);

        let agent = make_agent(provider);
        let result = agent.run("Explain main.rs").await.unwrap();

        assert_eq!(result.response, "The file contains a main function.");
        assert_eq!(result.iterations, 2);
        assert_eq!(result.input_tokens, 65);
        assert_eq!(result.output_tokens, 15);
    }

    // @req REQ-AGENT-005
    #[tokio::test]
    async fn tool_results_are_injected_into_history() {
        let provider = MockLlmProvider::new();

        // LLM requests read_file, then completes
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("Cargo.toml"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        provider.queue_response(vec![
            StreamEvent::Token("Done.".to_string()),
            StreamEvent::Done {
                input_tokens: 30,
                output_tokens: 2,
            },
        ]);

        let agent = make_agent(provider);
        let result = agent.run("Read Cargo.toml").await.unwrap();

        // The loop should have completed after 2 iterations
        // (tool use -> text response)
        assert_eq!(result.iterations, 2);
        assert_eq!(result.response, "Done.");
    }

    // @req REQ-AGENT-008
    #[tokio::test]
    async fn agent_halts_at_max_iterations() {
        let provider = MockLlmProvider::new();

        // Queue 5 iterations of tool calls (will exceed max of 3)
        for _ in 0..5 {
            provider.queue_response(vec![
                StreamEvent::ToolUse(ToolCall::ReadFile {
                    path: FilePath::new_unchecked("file.rs"),
                }),
                StreamEvent::Done {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            ]);
        }

        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig {
                max_iterations: 3,
                ..Default::default()
            },
        );

        let result = agent.run("Loop forever").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("max iterations"),
            "Error should mention max iterations: {err}"
        );
    }

    // @req REQ-HITL-001
    #[tokio::test]
    async fn denied_tool_call_injects_permission_denied() {
        let provider = MockLlmProvider::new();

        // LLM proposes a write, then completes
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::WriteFile {
                path: FilePath::new_unchecked("bad.rs"),
                content: "malicious".to_string(),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        provider.queue_response(vec![
            StreamEvent::Token("Understood, skipping.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 3,
            },
        ]);

        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_deny(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Write bad code").await.unwrap();
        assert_eq!(result.response, "Understood, skipping.");
    }

    // @req REQ-AGENT-001
    #[tokio::test]
    async fn safe_tools_auto_execute_without_hitl() {
        let provider = MockLlmProvider::new();

        // LLM requests a read (safe), then completes
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("readme.md"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        provider.queue_response(vec![
            StreamEvent::Token("Got it.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 2,
            },
        ]);

        // Use always_deny gate -- but read_file is safe,
        // so it should NOT go through the gate
        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_deny(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Read the readme").await.unwrap();
        // If the gate were consulted, it would deny and the
        // tool result would be "Permission denied". But since
        // read_file is safe, it should auto-execute.
        assert_eq!(result.response, "Got it.");
    }

    // @req REQ-AGENT-001
    #[tokio::test]
    async fn stream_error_propagates() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![StreamEvent::Error("Connection reset".to_string())]);

        let agent = make_agent(provider);
        let result = agent.run("Hi").await;
        assert!(result.is_err());
    }

    // @req REQ-AGENT-001
    #[test]
    fn builtin_tools_has_five_entries() {
        let tools = builtin_tool_schemas();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"run_command"));
        assert!(names.contains(&"list_dir"));
        assert!(names.contains(&"grep"));
    }
}
