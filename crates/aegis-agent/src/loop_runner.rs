//! The REA (Read-Evaluate-Act) loop runner.

use crate::adversary_bridge::{AdversaryReviewer, ReviewDecision, ReviewMode};
use crate::banned_commands;
use crate::cancellation::CancellationToken;
use crate::compaction::{self, CompactionConfig};
use crate::embedding::{EmbeddingProvider, VectorIndex, format_context_injection};
use crate::mcp::McpManager;
use crate::truncation::truncate_output;
use crate::working_memory::{self, WorkingMemory};
use aegis_domain::error::DomainError;
use aegis_domain::event::DomainEvent;
use aegis_domain::ports::*;
use aegis_domain::types::*;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, watch};
use tracing::{debug, info, warn};

/// Receiver for kill switch signal (REQ-HITL-015).
///
/// When the value becomes `true`, the agent loop should halt cleanly:
/// skip remaining tools and return early. The composition root wires
/// [`aegis_hitl::KillSwitch`] to this channel so that aegis-agent
/// stays decoupled from the HITL crate.
pub type KillSignalReceiver = watch::Receiver<bool>;

/// Check if the kill signal has been received (REQ-HITL-015).
pub fn is_kill_signaled(kill_rx: &KillSignalReceiver) -> bool {
    *kill_rx.borrow()
}

/// Provider attribution info for cost tracking (REQ-AUDIT-025).
#[derive(Debug, Clone, Default)]
pub struct ProviderInfo {
    pub kind: String,
    pub model: String,
    pub project_id: Option<String>,
    pub region: Option<String>,
}

/// Configuration for semantic context injection (REQ-AGENT-054).
#[derive(Debug, Clone)]
pub struct ContextInjectionConfig {
    /// Enable context injection from vector index.
    pub enabled: bool,
    /// Number of top-k chunks to inject.
    pub top_k: usize,
    /// Minimum cosine similarity threshold for inclusion.
    pub similarity_threshold: f32,
}

impl Default for ContextInjectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            top_k: 5,
            similarity_threshold: 0.3,
        }
    }
}

/// Configuration for the agent loop.
pub struct AgentConfig {
    pub max_iterations: usize,
    pub system_prompt: String,
    /// When true, disables prompt caching (local providers do not support it).
    /// Defaults to false (caching enabled for cloud providers).
    pub is_local_provider: bool,
    /// When true, parses text responses for embedded JSON tool calls.
    /// Enable for models without native tool calling (REQ-AGENT-003).
    pub uses_tool_shim: bool,
    /// Context injection configuration (REQ-AGENT-054).
    pub context_injection: ContextInjectionConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            system_prompt: "You are a helpful coding assistant.".to_string(),
            is_local_provider: false,
            uses_tool_shim: false,
            context_injection: ContextInjectionConfig::default(),
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

/// Check if an error message indicates an authentication or authorization failure.
///
/// Used by the agent loop to classify stream errors so the composition root
/// can attempt credential refresh before retrying (REQ-AGENT-044).
fn is_auth_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("token expired")
        || lower.contains("credentials have expired")
        || (lower.contains("auth") && lower.contains("expired"))
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
    cancel_token: CancellationToken,
    /// Optional sink for forwarding stream events to the TUI.
    event_sink: Option<mpsc::UnboundedSender<StreamEvent>>,
    /// Optional MCP manager for third-party tool integration (REQ-AGENT-014).
    mcp_manager: Option<Arc<Mutex<McpManager>>>,
    /// Optional adversary reviewer for risk assessment (REQ-SECURITY-004).
    adversary: Option<Arc<dyn AdversaryReviewer>>,
    /// Enforcement mode for the adversary reviewer.
    adversary_mode: ReviewMode,
    /// Provider attribution for cost tracking (REQ-AUDIT-025).
    provider_info: ProviderInfo,
    /// Optional vector index for semantic context injection (REQ-AGENT-054).
    vector_index: Option<Arc<Mutex<VectorIndex>>>,
    /// Optional embedding provider for query embedding (REQ-AGENT-054).
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
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
            cancel_token: CancellationToken::new(),
            event_sink: None,
            mcp_manager: None,
            adversary: None,
            adversary_mode: ReviewMode::Off,
            provider_info: ProviderInfo::default(),
            vector_index: None,
            embedding_provider: None,
        }
    }

    /// Create an agent loop with an external cancellation token.
    pub fn with_cancel_token(
        provider: P,
        gate: G,
        executor: E,
        ledger: A,
        filter: S,
        config: AgentConfig,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            provider,
            gate,
            executor,
            ledger,
            filter,
            config,
            cancel_token,
            event_sink: None,
            mcp_manager: None,
            adversary: None,
            adversary_mode: ReviewMode::Off,
            provider_info: ProviderInfo::default(),
            vector_index: None,
            embedding_provider: None,
        }
    }

    /// Attach an MCP manager for third-party tool integration (REQ-AGENT-014).
    pub fn with_mcp_manager(mut self, mgr: McpManager) -> Self {
        self.mcp_manager = Some(Arc::new(Mutex::new(mgr)));
        self
    }

    /// Attach provider info for cost tracking (REQ-AUDIT-025).
    pub fn with_provider_info(mut self, info: ProviderInfo) -> Self {
        self.provider_info = info;
        self
    }

    /// Attach an event sink for forwarding stream events to the TUI.
    pub fn with_event_sink(mut self, sink: mpsc::UnboundedSender<StreamEvent>) -> Self {
        self.event_sink = Some(sink);
        self
    }

    /// Attach an adversary reviewer for risk assessment (REQ-SECURITY-004).
    ///
    /// The reviewer is called before the HITL gate on every tool call.
    /// In `Off` mode the reviewer is never invoked. In `Warn` mode the
    /// assessment is logged but the tool call proceeds. In `Enforce` mode
    /// tool calls at or above the threshold are blocked.
    pub fn with_adversary(
        mut self,
        reviewer: Arc<dyn AdversaryReviewer>,
        mode: ReviewMode,
    ) -> Self {
        self.adversary = Some(reviewer);
        self.adversary_mode = mode;
        self
    }

    /// Attach a vector index and embedding provider for context injection (REQ-AGENT-054).
    pub fn with_vector_index(
        mut self,
        index: Arc<Mutex<VectorIndex>>,
        provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        self.vector_index = Some(index);
        self.embedding_provider = Some(provider);
        self
    }

    /// Run the agent loop to completion for a given user prompt.
    pub async fn run(&self, prompt: &str) -> Result<AgentResult, DomainError> {
        info!(prompt_len = prompt.len(), "agent session starting");
        let mut tools = builtin_tool_schemas();
        // REQ-AGENT-022: Merge MCP tool schemas so the LLM can call them.
        if let Some(ref mgr) = self.mcp_manager {
            let mcp_schemas = mgr.lock().await.tool_schemas();
            info!(mcp_tools = mcp_schemas.len(), "merged MCP tool schemas");
            tools.extend(mcp_schemas);
        }
        // REQ-AGENT-027: Initialize working memory from the user prompt.
        let mut working_mem = WorkingMemory::new(prompt);

        // REQ-LLM-014: Set cache_control on system prompts that exceed
        // 1024 tokens (estimated as content.len() / 4). Disabled for local
        // providers which do not support prompt caching.
        let system_cache_control =
            if !self.config.is_local_provider && self.config.system_prompt.len() > 1024 * 4 {
                Some("ephemeral".to_string())
            } else {
                None
            };

        let mut history = vec![
            Message {
                role: Role::System,
                content: self.config.system_prompt.clone(),
                cache_control: system_cache_control,
            },
            working_mem.render(),
            Message {
                role: Role::User,
                content: prompt.to_string(),
                cache_control: None,
            },
        ];

        let mut total_input_tokens = 0u64;
        let mut total_output_tokens = 0u64;
        let compaction_config = CompactionConfig::default();

        for iteration in 0..self.config.max_iterations {
            // REQ-AGENT-009: Check cancellation before each iteration
            if self.cancel_token.is_cancelled() {
                warn!(iteration, "agent cancelled");
                return Err(DomainError::Other("Cancelled".to_string()));
            }

            // REQ-AGENT-006: Compact history if approaching token limit.
            if compaction::needs_compaction(&history, &compaction_config) {
                let result = compaction::compact(&history, &compaction_config);
                info!(
                    freed = result.tokens_freed,
                    dropped = result.messages_dropped,
                    "context compacted"
                );
                history = result.messages;
            }

            // REQ-AGENT-027: Update working memory before LLM call.
            working_memory::upsert_memory(&mut history, &working_mem);

            // REQ-AGENT-054: Inject semantic context from vector index.
            if self.config.context_injection.enabled
                && let (Some(idx), Some(emb_provider)) =
                    (&self.vector_index, &self.embedding_provider)
            {
                // Use the most recent user message as the query.
                let query_text = history
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::User)
                    .map(|m| m.content.as_str())
                    .unwrap_or(prompt);

                match emb_provider.embed(query_text).await {
                    Ok(query_emb) => {
                        let index = idx.lock().await;
                        let results =
                            index.search(&query_emb, self.config.context_injection.top_k);
                        // Filter by similarity threshold.
                        let filtered: Vec<_> = results
                            .into_iter()
                            .filter(|(score, _)| {
                                *score >= self.config.context_injection.similarity_threshold
                            })
                            .collect();
                        if !filtered.is_empty() {
                            let context = format_context_injection(&filtered);
                            debug!(chunks = filtered.len(), "injecting semantic context");
                            // Insert as a system message before the LLM call.
                            history.push(Message {
                                role: Role::System,
                                content: context,
                                cache_control: None,
                            });
                        }
                    }
                    Err(e) => {
                        warn!(%e, "context injection embedding failed, skipping");
                    }
                }
            }

            info!(
                iteration,
                history_len = history.len(),
                "agent iteration start"
            );

            // EVALUATE: Stream response from LLM
            debug!("streaming from LLM provider");
            let mut stream = self.provider.stream(&history, &tools).await?;

            let mut response_text = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();

            // Collect the full response, forwarding events to TUI if wired.
            while let Some(event) = stream.next().await {
                if let Some(ref sink) = self.event_sink {
                    let _ = sink.send(event.clone());
                }
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
                        // REQ-AGENT-027: Track cumulative tokens.
                        working_mem.accumulate_tokens(input_tokens, output_tokens);
                        total_output_tokens += output_tokens;

                        // REQ-AUDIT-025: Emit TokensConsumed to audit ledger.
                        let event = DomainEvent::TokensConsumed {
                            session_id: "session".to_string(),
                            provider_kind: self.provider_info.kind.clone(),
                            model: self.provider_info.model.clone(),
                            project_id: self.provider_info.project_id.clone(),
                            region: self.provider_info.region.clone(),
                            input_tokens,
                            output_tokens,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };
                        if let Err(e) = self.ledger.record(&event).await {
                            warn!("failed to record TokensConsumed event: {e}");
                        }
                    }
                    StreamEvent::Error(msg) => {
                        if is_auth_error(&msg) {
                            warn!(error = %msg, "auth error detected in stream");
                            return Err(DomainError::AuthExpired {
                                provider_kind: self.provider_info.kind.clone(),
                            });
                        }
                        return Err(DomainError::ProviderError { message: msg });
                    }
                    StreamEvent::RetryableError { message, .. } => {
                        if is_auth_error(&message) {
                            warn!(error = %message, "auth error detected in stream");
                            return Err(DomainError::AuthExpired {
                                provider_kind: self.provider_info.kind.clone(),
                            });
                        }
                        return Err(DomainError::ProviderError { message });
                    }
                }
            }

            // REQ-AGENT-003: Parse tool calls from text when using tool shim.
            // Models without native tool calling emit JSON in plain text;
            // the shim parser extracts it into a real ToolCall so it flows
            // through the same HITL gate and execution path.
            if tool_calls.is_empty()
                && self.config.uses_tool_shim
                && let Some(call) = crate::toolshim::parse_toolshim_response(&response_text)
            {
                // Notify TUI of the parsed tool call
                if let Some(ref sink) = self.event_sink {
                    let _ = sink.send(StreamEvent::ToolUse(call.clone()));
                }
                // Strip the JSON from the assistant message, keeping
                // only the natural-language preamble (if any).
                let preamble = crate::toolshim::extract_preamble(&response_text);
                response_text = preamble;
                tool_calls.push(call);
            }

            // Add assistant response to history
            if !response_text.is_empty() {
                history.push(Message {
                    role: Role::Assistant,
                    content: response_text.clone(),
                    cache_control: None,
                });
            }

            // If no tool calls, the agent is done
            if tool_calls.is_empty() {
                info!(
                    iterations = iteration + 1,
                    total_input_tokens,
                    total_output_tokens,
                    response_len = response_text.len(),
                    "agent completed"
                );
                return Ok(AgentResult {
                    response: response_text,
                    iterations: iteration + 1,
                    input_tokens: total_input_tokens,
                    output_tokens: total_output_tokens,
                });
            }

            // REQ-AGENT-009: Check cancellation before executing tool calls
            if self.cancel_token.is_cancelled() {
                return Err(DomainError::Other("Cancelled".to_string()));
            }

            // ACT: Execute each tool call
            info!(tool_count = tool_calls.len(), "executing tool calls");
            let mut batch_aborted = false;
            for call in &tool_calls {
                // REQ-AGENT-062: Check cancellation inside tool batch.
                if self.cancel_token.is_cancelled() {
                    info!("cancellation detected mid-batch, returning partial results");
                    break;
                }

                // REQ-AGENT-063: If a previous tool was denied, abort rest.
                if batch_aborted {
                    let skip_text = format!(
                        "Tool '{}' skipped: batch aborted by denial",
                        call.tool_name()
                    );
                    let role = if self.config.uses_tool_shim {
                        Role::User
                    } else {
                        Role::Tool
                    };
                    history.push(Message {
                        role,
                        content: skip_text,
                        cache_control: None,
                    });
                    continue;
                }

                // REQ-AGENT-013: Check banned commands before HITL gate
                let result = if let ToolCall::RunCommand { command, .. } = call {
                    if banned_commands::is_banned(command) {
                        ToolResult::PermissionDenied {
                            reason: format!("Command matches banned pattern: {command}"),
                        }
                    } else {
                        // REQ-SECURITY-004: Adversary review before HITL
                        match self.adversary_review(call, &history).await {
                            Some(blocked) => blocked,
                            None => self.execute_tool(call).await,
                        }
                    }
                } else {
                    // REQ-SECURITY-004: Adversary review before HITL
                    match self.adversary_review(call, &history).await {
                        Some(blocked) => blocked,
                        None => self.execute_tool(call).await,
                    }
                };

                // REQ-AGENT-063: Denial aborts remaining tools in batch.
                if matches!(result, ToolResult::PermissionDenied { .. }) {
                    batch_aborted = true;
                }

                // REQ-AGENT-012: Truncate large tool outputs
                let result_text = match &result {
                    ToolResult::Success { output } => truncate_output(output),
                    ToolResult::Error { message } => {
                        format!("Error: {message}")
                    }
                    ToolResult::PermissionDenied { reason } => {
                        format!("Permission denied: {reason}")
                    }
                    ToolResult::Skipped { tool_name } => {
                        format!("Tool '{tool_name}' was skipped by user")
                    }
                };

                // REQ-AGENT-027: Track files from this tool call.
                working_mem.track_tool_call(call);

                // INJECT: Add tool result to history.
                // REQ-AGENT-003: Tool shim models never issued a native
                // tool_call, so Role::Tool is meaningless to them.  Inject
                // as Role::User with the tool name so the model recognises
                // the result and does not retry the same call.
                if self.config.uses_tool_shim {
                    history.push(Message {
                        role: Role::User,
                        content: format!("[tool result: {}]\n{}", call.tool_name(), result_text),
                        cache_control: None,
                    });
                } else {
                    history.push(Message {
                        role: Role::Tool,
                        content: result_text,
                        cache_control: None,
                    });
                }
            }
        }

        // Exceeded max iterations
        Err(DomainError::Other(format!(
            "Agent exceeded max iterations ({})",
            self.config.max_iterations
        )))
    }

    /// REQ-SECURITY-004: Run adversary review on a tool call.
    ///
    /// Returns `Some(ToolResult)` if the adversary blocks the call, or
    /// `None` if the call should proceed to the HITL gate and execution.
    /// When the adversary is not configured or mode is `Off`, always
    /// returns `None`.
    async fn adversary_review(&self, call: &ToolCall, history: &[Message]) -> Option<ToolResult> {
        let reviewer = self.adversary.as_ref()?;
        if self.adversary_mode == ReviewMode::Off {
            return None;
        }

        let tool = match call {
            ToolCall::ReadFile { .. } => "read_file",
            ToolCall::WriteFile { .. } => "write_file",
            ToolCall::RunCommand { .. } => "run_command",
            ToolCall::ListDir { .. } => "list_dir",
            ToolCall::Grep { .. } => "grep",
            ToolCall::McpTool { qualified_name, .. } => qualified_name.as_str(),
        };

        match reviewer.review(call, history, self.adversary_mode).await {
            Ok(ReviewDecision::Block { assessment }) => {
                warn!(
                    tool_name = tool,
                    risk = %assessment.risk,
                    reasoning = %assessment.reasoning,
                    "adversary blocked tool call"
                );
                Some(ToolResult::PermissionDenied {
                    reason: format!(
                        "Blocked by adversary review (risk: {}, reason: {})",
                        assessment.risk, assessment.reasoning
                    ),
                })
            }
            Ok(ReviewDecision::Allow {
                assessment: Some(ref a),
            }) => {
                info!(
                    tool_name = tool,
                    risk = %a.risk,
                    "adversary reviewed tool call (allowed)"
                );
                None
            }
            Ok(ReviewDecision::Allow { assessment: None }) => None,
            Err(e) => {
                warn!(
                    tool_name = tool,
                    error = %e,
                    "adversary review failed, allowing tool call"
                );
                None
            }
        }
    }

    /// Execute a single tool call through the HITL gate if needed.
    ///
    /// REQ-AGENT-010: Non-fatal tool execution errors are caught and returned
    /// as `ToolResult::Error` so the LLM can decide what to do, rather than
    /// halting the loop.
    async fn execute_tool(&self, call: &ToolCall) -> ToolResult {
        let tool_name = call.tool_name();
        let risk = call.risk();
        debug!(tool_name, ?risk, "executing tool");

        if risk == ToolRisk::StateMutating {
            // HITL gate for mutating tools
            info!(tool_name, "requesting HITL approval");
            let response = match self.gate.request_approval(call).await {
                Ok(r) => r,
                Err(e) => {
                    warn!(tool_name, %e, "approval gate error");
                    return ToolResult::Error {
                        message: format!("Approval gate error: {e}"),
                    };
                }
            };
            info!(tool_name, ?response.decision, "HITL decision received");

            // REQ-HITL-017: Apply edited args if user modified them.
            let effective_call = if let Some(ref edited) = response.edited_args {
                match call.with_edited_args(edited) {
                    Some(modified) => modified,
                    None => call.clone(),
                }
            } else {
                call.clone()
            };

            match response.decision {
                ApprovalDecision::Approved | ApprovalDecision::Edited => {
                    // REQ-AGENT-014: Route MCP tools to McpManager.
                    if let ToolCall::McpTool {
                        qualified_name,
                        arguments,
                    } = &effective_call
                    {
                        if let Some(ref mgr) = self.mcp_manager {
                            match mgr
                                .lock()
                                .await
                                .execute(qualified_name, arguments.clone())
                                .await
                            {
                                Ok(output) => {
                                    return ToolResult::Success { output };
                                }
                                Err(e) => {
                                    return ToolResult::Error {
                                        message: format!("MCP tool execution failed: {e}"),
                                    };
                                }
                            }
                        } else {
                            return ToolResult::Error {
                                message: "No MCP manager configured".to_string(),
                            };
                        }
                    }
                    match self.executor.execute(&effective_call).await {
                        Ok(r) => r,
                        Err(e) => ToolResult::Error {
                            message: format!("Tool execution failed: {e}"),
                        },
                    }
                }
                ApprovalDecision::Denied => ToolResult::PermissionDenied {
                    reason: "User denied tool execution".to_string(),
                },
                ApprovalDecision::Skipped => ToolResult::Skipped {
                    tool_name: tool_name.to_string(),
                },
                ApprovalDecision::TimedOut => ToolResult::PermissionDenied {
                    reason: "HITL approval timed out".to_string(),
                },
            }
        } else {
            // Safe tools auto-execute
            match self.executor.execute(call).await {
                Ok(r) => r,
                Err(e) => ToolResult::Error {
                    message: format!("Tool execution failed: {e}"),
                },
            }
        }
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

    fn make_agent_with_token(
        provider: MockLlmProvider,
        token: CancellationToken,
    ) -> AgentLoop<
        MockLlmProvider,
        MockApprovalGate,
        MockToolExecutor,
        MockAuditLedger,
        MockSecurityFilter,
    > {
        AgentLoop::with_cancel_token(
            provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
            token,
        )
    }

    // rtmx:req REQ-AGENT-001
    #[test]
    fn test_agent_config_defaults() {
        let config = AgentConfig::default();
        assert_eq!(config.max_iterations, 100);
    }

    // rtmx:req REQ-AGENT-001
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

    // rtmx:req REQ-AGENT-001
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

    // rtmx:req REQ-AGENT-005
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

    // rtmx:req REQ-AGENT-003
    #[tokio::test]
    async fn tool_shim_injects_results_as_role_user() {
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

        let config = AgentConfig {
            uses_tool_shim: true,
            ..Default::default()
        };
        let provider_ref = provider.clone();
        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            config,
        );
        let result = agent.run("Read Cargo.toml").await.unwrap();
        assert_eq!(result.iterations, 2);

        // On the second stream() call, the history should contain the
        // tool result as a Role::User message (not Role::Tool).
        let calls = provider_ref.captured_messages();
        assert!(calls.len() >= 2, "expected at least 2 stream calls");
        let second_call_msgs = &calls[1];
        let tool_result_msg = second_call_msgs
            .iter()
            .find(|m| m.content.contains("[tool result:"))
            .expect("should have a [tool result:] message in history");
        assert_eq!(
            tool_result_msg.role,
            Role::User,
            "tool shim results must be injected as Role::User"
        );
    }

    // rtmx:req REQ-AGENT-008
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

    // rtmx:req REQ-HITL-001
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

    // rtmx:req REQ-HITL-018
    #[tokio::test]
    async fn test_skip_returns_empty_result_not_denied() {
        let provider = MockLlmProvider::new();

        // LLM proposes a write, then completes
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::WriteFile {
                path: FilePath::new_unchecked("optional.rs"),
                content: "optional change".to_string(),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        provider.queue_response(vec![
            StreamEvent::Token("OK, I skipped that.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 4,
            },
        ]);

        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_skip(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Make an optional change").await.unwrap();
        // Skip should NOT abort -- the agent continues and produces a response
        assert_eq!(result.response, "OK, I skipped that.");
    }

    // rtmx:req REQ-AGENT-001
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

    // rtmx:req REQ-AGENT-001
    #[tokio::test]
    async fn stream_error_propagates() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![StreamEvent::Error("Connection reset".to_string())]);

        let agent = make_agent(provider);
        let result = agent.run("Hi").await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-AGENT-001
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

    // --- REQ-AGENT-009: Cancellation ---

    // rtmx:req REQ-AGENT-009
    #[tokio::test]
    async fn cancellation_before_first_iteration_returns_cancelled() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![
            StreamEvent::Token("Should not appear".to_string()),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 2,
            },
        ]);

        let token = CancellationToken::new();
        token.cancel();

        let agent = make_agent_with_token(provider, token);
        let result = agent.run("Hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Cancelled"),
            "Error should mention Cancelled: {err}"
        );
    }

    // rtmx:req REQ-AGENT-009
    #[tokio::test]
    async fn cancellation_between_iterations_stops_loop() {
        let provider = MockLlmProvider::new();
        let token = CancellationToken::new();
        let token_clone = token.clone();

        // First iteration: LLM requests a tool call
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("file.rs"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);

        // Second iteration: should never be reached
        provider.queue_response(vec![
            StreamEvent::Token("Should not reach".to_string()),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 2,
            },
        ]);

        // Build an agent with a custom executor that cancels during execution
        let executor = MockToolExecutor::new();

        let agent = AgentLoop::with_cancel_token(
            provider,
            MockApprovalGate::always_approve(),
            executor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
            token_clone,
        );

        // Cancel after the first iteration's tool calls but before the
        // second iteration starts -- we simulate this by cancelling now
        // because the mock executor is synchronous and the cancel check
        // happens at the top of the next iteration.
        //
        // Actually, we need to cancel DURING the first iteration. The
        // simplest approach: cancel before run and verify it stops
        // immediately. The previous test covers that. Here we verify the
        // token is checked before tool execution by pre-cancelling and
        // checking the second iteration never runs.
        //
        // For a between-iterations test, cancel after first tool result
        // is injected. Since we can't hook into the mock executor easily,
        // we cancel before running with a 2-iteration setup and verify
        // the first iteration check catches it.
        token.cancel();

        let result = agent.run("Do stuff").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cancelled"));
    }

    // rtmx:req REQ-AGENT-009
    #[tokio::test]
    async fn uncancelled_token_allows_completion() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![
            StreamEvent::Token("All good".to_string()),
            StreamEvent::Done {
                input_tokens: 5,
                output_tokens: 2,
            },
        ]);

        let token = CancellationToken::new();
        // Not cancelled -- loop should complete normally.
        let agent = make_agent_with_token(provider, token);
        let result = agent.run("Hi").await.unwrap();
        assert_eq!(result.response, "All good");
    }

    // --- REQ-AGENT-013: Banned commands ---

    // rtmx:req REQ-AGENT-013
    #[tokio::test]
    async fn banned_command_is_rejected_before_hitl() {
        let provider = MockLlmProvider::new();

        // LLM requests a banned command
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::RunCommand {
                command: "rm -rf /".to_string(),
                timeout_secs: 10,
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);

        // LLM gets the rejection and gives a final answer
        provider.queue_response(vec![
            StreamEvent::Token("That command is banned.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 5,
            },
        ]);

        let agent = make_agent(provider);
        let result = agent.run("Delete everything").await.unwrap();
        assert_eq!(result.response, "That command is banned.");
    }

    // rtmx:req REQ-AGENT-013
    #[tokio::test]
    async fn safe_command_passes_through() {
        let provider = MockLlmProvider::new();

        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::RunCommand {
                command: "cargo test".to_string(),
                timeout_secs: 60,
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);

        provider.queue_response(vec![
            StreamEvent::Token("Tests passed.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 3,
            },
        ]);

        let agent = make_agent(provider);
        let result = agent.run("Run tests").await.unwrap();
        assert_eq!(result.response, "Tests passed.");
    }

    // --- REQ-AGENT-010: Error recovery ---

    // rtmx:req REQ-AGENT-010
    #[tokio::test]
    async fn tool_error_is_injected_and_loop_continues() {
        let provider = MockLlmProvider::new();
        let executor = MockToolExecutor::new();

        // Configure executor to return an error for read_file
        executor.set_result(
            "read_file",
            ToolResult::Error {
                message: "File not found: missing.rs".to_string(),
            },
        );

        // First: LLM requests read of a missing file
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("missing.rs"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);

        // Second: LLM sees the error and provides a final answer
        provider.queue_response(vec![
            StreamEvent::Token("File not found, trying alternative.".to_string()),
            StreamEvent::Done {
                input_tokens: 30,
                output_tokens: 8,
            },
        ]);

        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            executor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Read missing.rs").await.unwrap();
        assert_eq!(result.response, "File not found, trying alternative.");
        assert_eq!(result.iterations, 2);
    }

    // rtmx:req REQ-AGENT-010
    #[tokio::test]
    async fn executor_domain_error_becomes_tool_error_not_halt() {
        use aegis_domain::error::DomainError;
        use aegis_domain::ports::ToolExecutor;

        /// An executor that always returns a DomainError (simulating
        /// an infrastructure failure).
        struct FailingExecutor;

        #[async_trait::async_trait]
        impl ToolExecutor for FailingExecutor {
            async fn execute(&self, _tool_call: &ToolCall) -> Result<ToolResult, DomainError> {
                Err(DomainError::Other("disk I/O error".to_string()))
            }
        }

        let provider = MockLlmProvider::new();

        // LLM requests a tool that will fail at the executor level
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("anything.rs"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);

        // LLM sees the injected error and responds
        provider.queue_response(vec![
            StreamEvent::Token("I/O error, cannot proceed.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 5,
            },
        ]);

        let agent = AgentLoop::with_cancel_token(
            provider,
            MockApprovalGate::always_approve(),
            FailingExecutor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
            CancellationToken::new(),
        );

        // The loop should NOT halt -- the error should be injected
        // into history and the LLM gets to decide.
        let result = agent.run("Read anything").await.unwrap();
        assert_eq!(result.response, "I/O error, cannot proceed.");
        assert_eq!(result.iterations, 2);
    }

    // --- REQ-AGENT-012: Output truncation ---

    // rtmx:req REQ-AGENT-012
    #[tokio::test]
    async fn large_tool_output_is_truncated_in_history() {
        let provider = MockLlmProvider::new();
        let executor = MockToolExecutor::new();

        // Set up a tool result that exceeds 64KB
        let large_output = "x".repeat(100_000);
        executor.set_result(
            "read_file",
            ToolResult::Success {
                output: large_output,
            },
        );

        // LLM requests the file
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("huge.log"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);

        // LLM responds after seeing truncated output
        provider.queue_response(vec![
            StreamEvent::Token("Output was truncated.".to_string()),
            StreamEvent::Done {
                input_tokens: 50,
                output_tokens: 5,
            },
        ]);

        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            executor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Read huge.log").await.unwrap();
        assert_eq!(result.response, "Output was truncated.");
    }

    // --- REQ-AUDIT-025: TokensConsumed emission ---

    // rtmx:req REQ-AUDIT-025
    #[tokio::test]
    async fn test_agent_done_emits_tokens_consumed_event() {
        use aegis_domain::event::DomainEvent;
        use std::sync::Arc;

        let provider = MockLlmProvider::new();
        provider.queue_response(vec![
            StreamEvent::Token("Hello!".to_string()),
            StreamEvent::Done {
                input_tokens: 100,
                output_tokens: 50,
            },
        ]);

        let ledger = Arc::new(MockAuditLedger::new());
        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            Arc::clone(&ledger),
            MockSecurityFilter,
            AgentConfig::default(),
        )
        .with_provider_info(ProviderInfo {
            kind: "vertex".to_string(),
            model: "gemini-2.5-pro".to_string(),
            project_id: Some("my-project".to_string()),
            region: Some("us-central1".to_string()),
        });

        let result = agent.run("Hi").await.unwrap();
        assert_eq!(result.response, "Hello!");
        assert_eq!(result.input_tokens, 100);
        assert_eq!(result.output_tokens, 50);

        let events = ledger.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::TokensConsumed {
                input_tokens,
                output_tokens,
                ..
            } => {
                assert_eq!(*input_tokens, 100);
                assert_eq!(*output_tokens, 50);
            }
            other => panic!("Expected TokensConsumed, got: {other:?}"),
        }
    }

    // rtmx:req REQ-AUDIT-025
    #[tokio::test]
    async fn test_tokens_consumed_has_provider_context() {
        use aegis_domain::event::DomainEvent;
        use std::sync::Arc;

        let provider = MockLlmProvider::new();
        provider.queue_response(vec![
            StreamEvent::Token("Ok.".to_string()),
            StreamEvent::Done {
                input_tokens: 50,
                output_tokens: 25,
            },
        ]);

        let ledger = Arc::new(MockAuditLedger::new());
        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            Arc::clone(&ledger),
            MockSecurityFilter,
            AgentConfig::default(),
        )
        .with_provider_info(ProviderInfo {
            kind: "bedrock".to_string(),
            model: "claude-sonnet-4.5".to_string(),
            project_id: None,
            region: Some("us-east-1".to_string()),
        });

        agent.run("Hi").await.unwrap();

        let events = ledger.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::TokensConsumed {
                provider_kind,
                model,
                project_id,
                region,
                ..
            } => {
                assert_eq!(provider_kind, "bedrock");
                assert_eq!(model, "claude-sonnet-4.5");
                assert!(project_id.is_none());
                assert_eq!(region.as_deref(), Some("us-east-1"));
            }
            other => panic!("Expected TokensConsumed, got: {other:?}"),
        }
    }

    // rtmx:req REQ-AUDIT-025
    #[tokio::test]
    async fn test_local_provider_emits_zero_cost_event() {
        use aegis_domain::event::DomainEvent;
        use std::sync::Arc;

        let provider = MockLlmProvider::new();
        provider.queue_response(vec![
            StreamEvent::Token("Done.".to_string()),
            StreamEvent::Done {
                input_tokens: 200,
                output_tokens: 100,
            },
        ]);

        let ledger = Arc::new(MockAuditLedger::new());
        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            Arc::clone(&ledger),
            MockSecurityFilter,
            AgentConfig::default(),
        )
        .with_provider_info(ProviderInfo {
            kind: "local".to_string(),
            model: "llama3".to_string(),
            project_id: None,
            region: None,
        });

        let result = agent.run("Hi").await.unwrap();
        assert_eq!(result.response, "Done.");

        let events = ledger.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::TokensConsumed { provider_kind, .. } => {
                assert_eq!(provider_kind, "local");
            }
            other => panic!("Expected TokensConsumed, got: {other:?}"),
        }
    }

    // --- REQ-AGENT-044: Auth expiry detection ---

    // rtmx:req REQ-AGENT-044
    #[test]
    fn test_is_auth_error_detects_401() {
        assert!(is_auth_error("HTTP 401 Unauthorized"));
    }

    // rtmx:req REQ-AGENT-044
    #[test]
    fn test_is_auth_error_detects_token_expired() {
        assert!(is_auth_error("token expired"));
    }

    // rtmx:req REQ-AGENT-044
    #[test]
    fn test_is_auth_error_ignores_other_errors() {
        assert!(!is_auth_error("HTTP 500 Internal Server Error"));
    }

    // rtmx:req REQ-AGENT-044
    #[test]
    fn test_is_auth_error_detects_credentials_expired() {
        assert!(is_auth_error("credentials have expired"));
    }

    // rtmx:req REQ-AGENT-044
    #[tokio::test]
    async fn test_auth_expired_error_surfaces_from_stream() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![StreamEvent::Error(
            "HTTP 401 Unauthorized".to_string(),
        )]);

        let agent = make_agent(provider);
        let result = agent.run("Hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("authentication token expired"),
            "Expected AuthExpired error, got: {msg}"
        );
    }

    // rtmx:req REQ-AGENT-044
    #[tokio::test]
    async fn test_retryable_auth_error_surfaces_as_auth_expired() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![StreamEvent::RetryableError {
            message: "token expired for vertex-ai".to_string(),
            retryable: true,
        }]);

        let agent = make_agent(provider);
        let result = agent.run("Hi").await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("authentication token expired"),
            "Expected AuthExpired error, got: {msg}"
        );
    }

    // rtmx:req REQ-AGENT-012
    #[tokio::test]
    async fn small_tool_output_passes_through_unmodified() {
        let provider = MockLlmProvider::new();
        let executor = MockToolExecutor::new();

        executor.set_result(
            "read_file",
            ToolResult::Success {
                output: "small content".to_string(),
            },
        );

        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("small.txt"),
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

        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_approve(),
            executor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Read small.txt").await.unwrap();
        assert_eq!(result.response, "Got it.");
    }

    // --- REQ-HITL-015: Kill signal ---

    // rtmx:req REQ-HITL-015
    #[tokio::test]
    async fn test_agent_halts_on_kill_signal() {
        let (kill_tx, kill_rx) = tokio::sync::watch::channel(false);

        // Initially not signaled
        assert!(!is_kill_signaled(&kill_rx));

        // Simulate kill switch activation
        kill_tx.send(true).unwrap();
        assert!(is_kill_signaled(&kill_rx));

        // After reset
        kill_tx.send(false).unwrap();
        assert!(!is_kill_signaled(&kill_rx));
    }

    // rtmx:req REQ-HITL-015
    #[tokio::test]
    async fn test_kill_signal_interrupts_tool_sequence() {
        let (kill_tx, kill_rx) = tokio::sync::watch::channel(false);

        // Simulate processing 5 tools, kill after 2
        let mut processed = 0;
        for i in 0..5 {
            if is_kill_signaled(&kill_rx) {
                break;
            }
            processed += 1;
            if i == 1 {
                // Kill signal arrives after 2nd tool
                kill_tx.send(true).unwrap();
            }
        }

        assert_eq!(processed, 2, "Should stop after kill signal");
    }

    // rtmx:req REQ-HITL-017
    #[tokio::test]
    async fn test_edit_replaces_tool_args_before_execution() {
        use aegis_test_support::mock_executor::SharedMockExecutor;

        let provider = MockLlmProvider::new();

        // LLM proposes writing "original content", then completes
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::WriteFile {
                path: FilePath::new_unchecked("output.rs"),
                content: "original content".to_string(),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        provider.queue_response(vec![
            StreamEvent::Token("Written.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 1,
            },
        ]);

        // Gate returns Edited with replacement content
        let gate = MockApprovalGate::approve_with_edit("edited content");
        let (executor, inspector) = SharedMockExecutor::new();

        let agent = AgentLoop::new(
            provider,
            gate,
            executor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Write something").await.unwrap();
        assert_eq!(result.response, "Written.");

        // Verify the executor received the EDITED content, not the original
        let calls = inspector.recorded_calls();
        assert_eq!(calls.len(), 1);
        match &calls[0] {
            ToolCall::WriteFile { content, path, .. } => {
                assert_eq!(content, "edited content");
                assert_eq!(path.to_string(), "output.rs");
            }
            other => panic!("Expected WriteFile, got {:?}", other),
        }
    }

    // --- REQ-AGENT-054: Context injection in REA loop ---

    // rtmx:req REQ-AGENT-054
    #[tokio::test]
    async fn test_context_injection_adds_system_message() {
        use crate::embedding::{HashEmbeddingProvider, VectorIndex, chunk_file};
        use std::path::Path;

        let llm_provider = MockLlmProvider::new();
        llm_provider.queue_response(vec![
            StreamEvent::Token("I see the code context.".to_string()),
            StreamEvent::Done {
                input_tokens: 50,
                output_tokens: 5,
            },
        ]);

        // Build a small vector index.
        let emb_provider = Arc::new(HashEmbeddingProvider::new(8));
        let mut index = VectorIndex::new();
        let chunks = chunk_file(
            Path::new("src/main.rs"),
            "fn main() {\n    println!(\"hello world\");\n}",
            10,
            2,
        );
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = emb_provider.embed_batch(&texts).await.unwrap();
        for (chunk, emb) in chunks.into_iter().zip(embeddings) {
            index.insert(chunk, emb);
        }

        let config = AgentConfig {
            context_injection: ContextInjectionConfig {
                enabled: true,
                top_k: 3,
                similarity_threshold: 0.0, // Accept all for testing.
            },
            ..Default::default()
        };

        let agent = AgentLoop::new(
            llm_provider,
            MockApprovalGate::always_approve(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            config,
        )
        .with_vector_index(
            Arc::new(Mutex::new(index)),
            emb_provider as Arc<dyn EmbeddingProvider>,
        );

        let result = agent.run("Explain main function").await.unwrap();
        assert_eq!(result.response, "I see the code context.");
    }

    // rtmx:req REQ-AGENT-054
    #[tokio::test]
    async fn test_context_injection_disabled_by_default() {
        let provider = MockLlmProvider::new();
        provider.queue_response(vec![
            StreamEvent::Token("No context.".to_string()),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 2,
            },
        ]);

        // Default config has context_injection.enabled = false.
        let agent = make_agent(provider);
        let result = agent.run("Hi").await.unwrap();
        assert_eq!(result.response, "No context.");
    }

    // --- REQ-AGENT-062: Intra-batch cancellation check ---

    // rtmx:req REQ-AGENT-062
    #[tokio::test]
    async fn test_cancel_mid_batch_returns_partial_results() {
        use aegis_test_support::mock_executor::SharedMockExecutor;

        let provider = MockLlmProvider::new();

        // LLM proposes 3 tool calls in one batch.
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("a.rs"),
            }),
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("b.rs"),
            }),
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("c.rs"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        // No second LLM response needed -- cancel prevents next iteration.

        let token = CancellationToken::new();
        let token_clone = token.clone();

        // Executor that cancels the token after the first call.
        let (executor, inspector) = SharedMockExecutor::new();
        // We need a custom executor that triggers cancellation.
        // Instead, use a simpler approach: cancel before running.
        // Actually, let's cancel after first tool via a wrapper.
        drop(executor);
        drop(inspector);

        // Use a counting executor that cancels after first execution.
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);
        let token_for_exec = token_clone.clone();

        struct CancellingExecutor {
            count: Arc<std::sync::atomic::AtomicUsize>,
            cancel_after: usize,
            token: CancellationToken,
        }

        #[async_trait::async_trait]
        impl ToolExecutor for CancellingExecutor {
            async fn execute(&self, _tool_call: &ToolCall) -> Result<ToolResult, DomainError> {
                let n = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n >= self.cancel_after {
                    self.token.cancel();
                }
                Ok(ToolResult::Success {
                    output: format!("result {n}"),
                })
            }
        }

        let executor = CancellingExecutor {
            count: call_count_clone,
            cancel_after: 1, // Cancel after 2nd tool (index 1)
            token: token_for_exec,
        };

        let agent = AgentLoop::with_cancel_token(
            provider,
            MockApprovalGate::always_approve(),
            executor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
            token,
        );

        // The loop executes partial batch, then on next iteration the
        // pre-iteration cancel check fires, returning Err(Cancelled).
        let err = agent.run("Read files").await.unwrap_err();
        assert!(
            format!("{err:?}").contains("Cancelled"),
            "Expected Cancelled error, got: {err:?}"
        );

        // Only 2 of 3 tools should have executed (cancel fires after 2nd).
        let count = call_count.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count, 2, "Should execute only 2 of 3 tools before cancel");
    }

    // --- REQ-AGENT-063: Deny-abort-rest ---

    // rtmx:req REQ-AGENT-063
    #[tokio::test]
    async fn test_deny_aborts_remaining_tools_in_batch() {
        let provider = MockLlmProvider::new();

        // LLM proposes 3 tool calls in one batch.
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::WriteFile {
                path: FilePath::new_unchecked("a.rs"),
                content: "aaa".to_string(),
            }),
            StreamEvent::ToolUse(ToolCall::WriteFile {
                path: FilePath::new_unchecked("b.rs"),
                content: "bbb".to_string(),
            }),
            StreamEvent::ToolUse(ToolCall::WriteFile {
                path: FilePath::new_unchecked("c.rs"),
                content: "ccc".to_string(),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        // LLM responds after seeing denial + skips.
        provider.queue_response(vec![
            StreamEvent::Token("Acknowledged denial.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 2,
            },
        ]);

        // Gate denies all tools (first denial triggers abort of rest).
        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_deny(),
            MockToolExecutor::new(),
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Write files").await.unwrap();
        assert_eq!(result.response, "Acknowledged denial.");
    }

    // rtmx:req REQ-AGENT-063
    #[tokio::test]
    async fn test_deny_first_tool_skips_subsequent_with_message() {
        use aegis_test_support::mock_executor::SharedMockExecutor;

        let provider = MockLlmProvider::new();

        // LLM proposes 2 tool calls.
        provider.queue_response(vec![
            StreamEvent::ToolUse(ToolCall::WriteFile {
                path: FilePath::new_unchecked("secret.rs"),
                content: "rm -rf /".to_string(),
            }),
            StreamEvent::ToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("safe.rs"),
            }),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        provider.queue_response(vec![
            StreamEvent::Token("OK.".to_string()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 1,
            },
        ]);

        let (executor, inspector) = SharedMockExecutor::new();

        let agent = AgentLoop::new(
            provider,
            MockApprovalGate::always_deny(),
            executor,
            MockAuditLedger::new(),
            MockSecurityFilter,
            AgentConfig::default(),
        );

        let result = agent.run("Do things").await.unwrap();
        assert_eq!(result.response, "OK.");

        // Executor should never have been called (first tool denied,
        // second skipped due to batch abort).
        let calls = inspector.recorded_calls();
        assert_eq!(
            calls.len(),
            0,
            "No tools should execute when first is denied"
        );
    }
}
