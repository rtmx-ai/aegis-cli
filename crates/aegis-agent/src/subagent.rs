//! Sub-agent spawning for parallel read-only tasks (REQ-AGENT-004).
//!
//! Sub-agents are lightweight task units that can execute read-only operations
//! in parallel. Each sub-agent has a restricted tool set (read-only by default)
//! and a configurable iteration limit.

use std::sync::Arc;

use aegis_domain::error::DomainError;
use aegis_domain::ports::{LlmProvider, Message, Role, SecurityFilter, StreamEvent, ToolSchema};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Token usage from a sub-agent execution (REQ-AGENT-021).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubAgentUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// A sub-agent that executes a single task with restricted tools.
#[derive(Debug)]
pub struct SubAgent {
    pub id: String,
    pub task: String,
    pub status: SubAgentStatus,
    /// Token usage recorded on completion (REQ-AGENT-021).
    pub usage: Option<SubAgentUsage>,
}

/// The lifecycle status of a sub-agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubAgentStatus {
    /// The sub-agent is currently executing.
    Running,
    /// The sub-agent completed successfully with a result.
    Completed(String),
    /// The sub-agent failed with an error message.
    Failed(String),
}

/// Configuration for a sub-agent's execution constraints.
#[derive(Debug, Clone)]
pub struct SubAgentConfig {
    /// Tool restrictions for the sub-agent (read-only tools only).
    pub allowed_tools: Vec<String>,
    /// Maximum iterations before timeout.
    pub max_iterations: usize,
    /// Whether the sub-agent can spawn its own sub-agents.
    pub allow_nesting: bool,
}

/// Mutating tool names that sub-agents must not use (REQ-AGENT-020).
const MUTATING_TOOLS: &[&str] = &["write_file", "run_command"];

impl SubAgentConfig {
    /// Validate that the allowed tool set contains no mutating tools.
    /// Sub-agents should default to read-only tools for safety
    /// (REQ-AGENT-020).
    pub fn validate_read_only(&self) -> Result<(), SpawnError> {
        for tool in &self.allowed_tools {
            if MUTATING_TOOLS.contains(&tool.as_str()) {
                return Err(SpawnError {
                    message: format!(
                        "sub-agent tool set must be read-only, \
                         but '{}' is mutating",
                        tool
                    ),
                });
            }
        }
        Ok(())
    }
}

impl Default for SubAgentConfig {
    fn default() -> Self {
        Self {
            allowed_tools: vec![
                "read_file".to_string(),
                "list_directory".to_string(),
                "search_files".to_string(),
            ],
            max_iterations: 10,
            allow_nesting: false,
        }
    }
}

/// Error returned when a sub-agent cannot be spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnError {
    pub message: String,
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubAgent spawn error: {}", self.message)
    }
}

impl std::error::Error for SpawnError {}

/// Manages the lifecycle of sub-agents.
pub struct SubAgentManager {
    agents: Vec<SubAgent>,
    max_concurrent: usize,
}

impl SubAgentManager {
    /// Create a new manager with the given concurrency limit.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            agents: Vec::new(),
            max_concurrent,
        }
    }

    /// Spawn a new sub-agent for the given task. Returns the agent ID on
    /// success, or a `SpawnError` if the concurrency limit is reached.
    pub fn spawn(&mut self, task: String, _config: SubAgentConfig) -> Result<String, SpawnError> {
        if self.active_count() >= self.max_concurrent {
            return Err(SpawnError {
                message: format!(
                    "concurrency limit reached ({}/{})",
                    self.active_count(),
                    self.max_concurrent
                ),
            });
        }

        let id = Uuid::new_v4().to_string();
        self.agents.push(SubAgent {
            id: id.clone(),
            task,
            status: SubAgentStatus::Running,
            usage: None,
        });
        Ok(id)
    }

    /// Query the status of a sub-agent by ID.
    pub fn status(&self, id: &str) -> Option<&SubAgentStatus> {
        self.agents.iter().find(|a| a.id == id).map(|a| &a.status)
    }

    /// Return the number of currently running sub-agents.
    pub fn active_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|a| a.status == SubAgentStatus::Running)
            .count()
    }

    /// Mark a sub-agent as completed with the given result.
    pub fn complete(&mut self, id: &str, result: String) -> bool {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.id == id) {
            agent.status = SubAgentStatus::Completed(result);
            true
        } else {
            false
        }
    }

    /// Mark a sub-agent as completed with the given result and token
    /// usage (REQ-AGENT-021).
    pub fn complete_with_usage(
        &mut self,
        id: &str,
        result: String,
        usage: SubAgentUsage,
    ) -> bool {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.id == id) {
            agent.status = SubAgentStatus::Completed(result);
            agent.usage = Some(usage);
            true
        } else {
            false
        }
    }

    /// Return the total token usage summed across all completed
    /// sub-agents (REQ-AGENT-021). Running or failed agents without
    /// usage data are excluded.
    pub fn total_usage(&self) -> SubAgentUsage {
        let mut total = SubAgentUsage::default();
        for agent in &self.agents {
            if let Some(ref u) = agent.usage {
                total.input_tokens += u.input_tokens;
                total.output_tokens += u.output_tokens;
            }
        }
        total
    }

    /// Mark a sub-agent as failed with the given error message.
    pub fn fail(&mut self, id: &str, error: String) -> bool {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.id == id) {
            agent.status = SubAgentStatus::Failed(error);
            true
        } else {
            false
        }
    }

    /// Drain and return all completed (or failed) sub-agents, leaving
    /// running agents in place.
    pub fn collect_completed(&mut self) -> Vec<SubAgent> {
        let mut completed = Vec::new();
        let mut remaining = Vec::new();

        for agent in self.agents.drain(..) {
            if agent.status == SubAgentStatus::Running {
                remaining.push(agent);
            } else {
                completed.push(agent);
            }
        }

        self.agents = remaining;
        completed
    }

    /// Spawn a sub-agent as an async task that runs an agent loop.
    /// Returns a tuple of (sub-agent ID, JoinHandle) on success.
    ///
    /// The sub-agent runs in a background tokio task with restricted
    /// (read-only) tools and no HITL gate. It communicates results back
    /// through the JoinHandle.
    pub async fn spawn_async(
        &mut self,
        config: SubAgentConfig,
        prompt: String,
        provider: Arc<dyn LlmProvider>,
        _filter: Arc<dyn SecurityFilter>,
    ) -> Result<(String, tokio::task::JoinHandle<Result<String, DomainError>>), SpawnError> {
        // Validate read-only tool set.
        config.validate_read_only()?;

        // Check concurrency limit (reuse existing spawn logic).
        if self.active_count() >= self.max_concurrent {
            return Err(SpawnError {
                message: format!(
                    "concurrency limit reached ({}/{})",
                    self.active_count(),
                    self.max_concurrent
                ),
            });
        }

        let id = Uuid::new_v4().to_string();
        self.agents.push(SubAgent {
            id: id.clone(),
            task: prompt.clone(),
            status: SubAgentStatus::Running,
            usage: None,
        });

        let handle = tokio::spawn(run_subagent_loop(provider, config, prompt));

        Ok((id, handle))
    }
}

/// Run a minimal agent loop for a sub-agent.
///
/// Uses restricted tools and has no HITL gate (sub-agents are read-only).
/// Loops up to `config.max_iterations` times, accumulating text output
/// from the LLM. Tool calls are not executed (sub-agents only produce
/// text summaries from the LLM's knowledge and the prompt context).
async fn run_subagent_loop(
    provider: Arc<dyn LlmProvider>,
    config: SubAgentConfig,
    prompt: String,
) -> Result<String, DomainError> {
    let system_msg = Message {
        role: Role::System,
        content: format!(
            "You are a read-only sub-agent. You may only use these tools: {}. \
             Summarize your findings as text.",
            config.allowed_tools.join(", ")
        ),
        cache_control: None,
    };
    let user_msg = Message {
        role: Role::User,
        content: prompt,
        cache_control: None,
    };

    let tool_schemas: Vec<ToolSchema> = config
        .allowed_tools
        .iter()
        .map(|name| ToolSchema {
            name: name.clone(),
            description: format!("Read-only tool: {name}"),
            parameters: serde_json::json!({}),
        })
        .collect();

    let mut accumulated = String::new();
    let messages = vec![system_msg, user_msg];

    for _iteration in 0..config.max_iterations {
        let mut stream = provider.stream(&messages, &tool_schemas).await?;

        let mut got_done = false;
        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::Token(text) => {
                    accumulated.push_str(&text);
                }
                StreamEvent::Done { .. } => {
                    got_done = true;
                    break;
                }
                StreamEvent::Error(msg) => {
                    return Err(DomainError::ProviderError { message: msg });
                }
                StreamEvent::RetryableError {
                    message,
                    retryable: _,
                } => {
                    return Err(DomainError::ProviderError { message });
                }
                StreamEvent::ToolUse(_) => {
                    // Sub-agent does not execute tools in this minimal
                    // loop; the LLM response is treated as text-only.
                    continue;
                }
            }
        }

        // If we received a Done event, the sub-agent is finished.
        if got_done {
            break;
        }
    }

    Ok(accumulated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_test_support::mock_filter::MockSecurityFilter;
    use aegis_test_support::mock_provider::MockLlmProvider;

    // rtmx:req REQ-AGENT-004
    #[test]
    fn default_config_has_read_only_tools() {
        let config = SubAgentConfig::default();
        assert_eq!(
            config.allowed_tools,
            vec!["read_file", "list_directory", "search_files"]
        );
        // Verify no mutating tools are present
        assert!(
            !config.allowed_tools.contains(&"write_file".to_string()),
            "default config must not include write_file"
        );
        assert!(
            !config.allowed_tools.contains(&"run_command".to_string()),
            "default config must not include run_command"
        );
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn default_config_max_iterations_is_10() {
        let config = SubAgentConfig::default();
        assert_eq!(config.max_iterations, 10);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn default_config_disallows_nesting() {
        let config = SubAgentConfig::default();
        assert!(!config.allow_nesting);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn manager_new_sets_max_concurrent() {
        let mgr = SubAgentManager::new(4);
        assert_eq!(mgr.max_concurrent, 4);
        assert_eq!(mgr.active_count(), 0);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn spawn_creates_agent_in_running_state() {
        let mut mgr = SubAgentManager::new(4);
        let id = mgr
            .spawn("analyze code".to_string(), SubAgentConfig::default())
            .unwrap();

        assert!(!id.is_empty());
        assert_eq!(mgr.status(&id), Some(&SubAgentStatus::Running));
        assert_eq!(mgr.active_count(), 1);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn spawn_returns_unique_ids() {
        let mut mgr = SubAgentManager::new(4);
        let id1 = mgr
            .spawn("task 1".to_string(), SubAgentConfig::default())
            .unwrap();
        let id2 = mgr
            .spawn("task 2".to_string(), SubAgentConfig::default())
            .unwrap();
        assert_ne!(id1, id2);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn spawn_respects_max_concurrent_limit() {
        let mut mgr = SubAgentManager::new(2);
        mgr.spawn("task 1".to_string(), SubAgentConfig::default())
            .unwrap();
        mgr.spawn("task 2".to_string(), SubAgentConfig::default())
            .unwrap();

        let result = mgr.spawn("task 3".to_string(), SubAgentConfig::default());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("concurrency limit"),
            "error should mention concurrency limit: {}",
            err.message
        );
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn spawn_allowed_after_agent_completes() {
        let mut mgr = SubAgentManager::new(1);
        let id = mgr
            .spawn("task 1".to_string(), SubAgentConfig::default())
            .unwrap();

        // At limit
        assert!(
            mgr.spawn("task 2".to_string(), SubAgentConfig::default())
                .is_err()
        );

        // Complete the first agent
        mgr.complete(&id, "done".to_string());

        // Now we should be able to spawn again
        let id2 = mgr
            .spawn("task 2".to_string(), SubAgentConfig::default())
            .unwrap();
        assert_eq!(mgr.status(&id2), Some(&SubAgentStatus::Running));
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn status_returns_none_for_unknown_id() {
        let mgr = SubAgentManager::new(4);
        assert_eq!(mgr.status("nonexistent"), None);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn status_returns_correct_state_after_completion() {
        let mut mgr = SubAgentManager::new(4);
        let id = mgr
            .spawn("task".to_string(), SubAgentConfig::default())
            .unwrap();

        mgr.complete(&id, "result text".to_string());
        assert_eq!(
            mgr.status(&id),
            Some(&SubAgentStatus::Completed("result text".to_string()))
        );
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn status_returns_correct_state_after_failure() {
        let mut mgr = SubAgentManager::new(4);
        let id = mgr
            .spawn("task".to_string(), SubAgentConfig::default())
            .unwrap();

        mgr.fail(&id, "something went wrong".to_string());
        assert_eq!(
            mgr.status(&id),
            Some(&SubAgentStatus::Failed("something went wrong".to_string()))
        );
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn collect_completed_drains_finished_agents() {
        let mut mgr = SubAgentManager::new(4);
        let id1 = mgr
            .spawn("task 1".to_string(), SubAgentConfig::default())
            .unwrap();
        let id2 = mgr
            .spawn("task 2".to_string(), SubAgentConfig::default())
            .unwrap();
        let _id3 = mgr
            .spawn("task 3".to_string(), SubAgentConfig::default())
            .unwrap();

        // Complete first, fail second, leave third running
        mgr.complete(&id1, "result 1".to_string());
        mgr.fail(&id2, "error 2".to_string());

        let completed = mgr.collect_completed();
        assert_eq!(completed.len(), 2);
        assert!(completed.iter().any(|a| a.id == id1));
        assert!(completed.iter().any(|a| a.id == id2));

        // Only the running agent should remain
        assert_eq!(mgr.active_count(), 1);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn collect_completed_returns_empty_when_all_running() {
        let mut mgr = SubAgentManager::new(4);
        mgr.spawn("task 1".to_string(), SubAgentConfig::default())
            .unwrap();
        mgr.spawn("task 2".to_string(), SubAgentConfig::default())
            .unwrap();

        let completed = mgr.collect_completed();
        assert!(completed.is_empty());
        assert_eq!(mgr.active_count(), 2);
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn multiple_agents_tracked_simultaneously() {
        let mut mgr = SubAgentManager::new(5);
        let mut ids = Vec::new();

        for i in 0..5 {
            let id = mgr
                .spawn(format!("task {i}"), SubAgentConfig::default())
                .unwrap();
            ids.push(id);
        }

        assert_eq!(mgr.active_count(), 5);

        // All should be running
        for id in &ids {
            assert_eq!(mgr.status(id), Some(&SubAgentStatus::Running));
        }

        // Complete some, fail some
        mgr.complete(&ids[0], "done 0".to_string());
        mgr.complete(&ids[2], "done 2".to_string());
        mgr.fail(&ids[4], "error 4".to_string());

        assert_eq!(mgr.active_count(), 2);

        // Verify individual states
        assert_eq!(
            mgr.status(&ids[0]),
            Some(&SubAgentStatus::Completed("done 0".to_string()))
        );
        assert_eq!(mgr.status(&ids[1]), Some(&SubAgentStatus::Running));
        assert_eq!(
            mgr.status(&ids[2]),
            Some(&SubAgentStatus::Completed("done 2".to_string()))
        );
        assert_eq!(mgr.status(&ids[3]), Some(&SubAgentStatus::Running));
        assert_eq!(
            mgr.status(&ids[4]),
            Some(&SubAgentStatus::Failed("error 4".to_string()))
        );
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn complete_returns_false_for_unknown_id() {
        let mut mgr = SubAgentManager::new(4);
        assert!(!mgr.complete("nonexistent", "result".to_string()));
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn fail_returns_false_for_unknown_id() {
        let mut mgr = SubAgentManager::new(4);
        assert!(!mgr.fail("nonexistent", "error".to_string()));
    }

    // rtmx:req REQ-AGENT-004
    #[test]
    fn spawn_error_display() {
        let err = SpawnError {
            message: "limit reached".to_string(),
        };
        assert_eq!(err.to_string(), "SubAgent spawn error: limit reached");
    }

    // --- REQ-AGENT-020: Tool set restriction validation ---

    // rtmx:req REQ-AGENT-020
    #[test]
    fn validate_read_only_rejects_write_file() {
        let config = SubAgentConfig {
            allowed_tools: vec!["read_file".to_string(), "write_file".to_string()],
            ..SubAgentConfig::default()
        };
        let err = config.validate_read_only().unwrap_err();
        assert!(
            err.message.contains("write_file"),
            "error should mention write_file: {}",
            err.message
        );
    }

    // rtmx:req REQ-AGENT-020
    #[test]
    fn validate_read_only_rejects_run_command() {
        let config = SubAgentConfig {
            allowed_tools: vec!["read_file".to_string(), "run_command".to_string()],
            ..SubAgentConfig::default()
        };
        let err = config.validate_read_only().unwrap_err();
        assert!(
            err.message.contains("run_command"),
            "error should mention run_command: {}",
            err.message
        );
    }

    // rtmx:req REQ-AGENT-020
    #[test]
    fn validate_read_only_accepts_read_tools() {
        let config = SubAgentConfig {
            allowed_tools: vec![
                "read_file".to_string(),
                "list_dir".to_string(),
                "grep".to_string(),
            ],
            ..SubAgentConfig::default()
        };
        assert!(config.validate_read_only().is_ok());
    }

    // --- REQ-AGENT-021: Sub-agent cost aggregation ---

    // rtmx:req REQ-AGENT-021
    #[test]
    fn complete_with_usage_records_tokens() {
        let mut mgr = SubAgentManager::new(4);
        let id = mgr
            .spawn("task".to_string(), SubAgentConfig::default())
            .unwrap();

        let usage = SubAgentUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        assert!(mgr.complete_with_usage(&id, "done".to_string(), usage));

        let agent = mgr.agents.iter().find(|a| a.id == id).unwrap();
        let u = agent.usage.as_ref().unwrap();
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
    }

    // rtmx:req REQ-AGENT-021
    #[test]
    fn total_usage_sums_completed() {
        let mut mgr = SubAgentManager::new(4);
        let id1 = mgr
            .spawn("t1".to_string(), SubAgentConfig::default())
            .unwrap();
        let id2 = mgr
            .spawn("t2".to_string(), SubAgentConfig::default())
            .unwrap();
        let id3 = mgr
            .spawn("t3".to_string(), SubAgentConfig::default())
            .unwrap();

        mgr.complete_with_usage(
            &id1,
            "r1".to_string(),
            SubAgentUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        );
        mgr.complete_with_usage(
            &id2,
            "r2".to_string(),
            SubAgentUsage {
                input_tokens: 200,
                output_tokens: 80,
            },
        );
        mgr.complete_with_usage(
            &id3,
            "r3".to_string(),
            SubAgentUsage {
                input_tokens: 50,
                output_tokens: 20,
            },
        );

        let total = mgr.total_usage();
        assert_eq!(total.input_tokens, 350);
        assert_eq!(total.output_tokens, 150);
    }

    // rtmx:req REQ-AGENT-021
    #[test]
    fn total_usage_ignores_running() {
        let mut mgr = SubAgentManager::new(4);
        let id1 = mgr
            .spawn("t1".to_string(), SubAgentConfig::default())
            .unwrap();
        let _id2 = mgr
            .spawn("t2".to_string(), SubAgentConfig::default())
            .unwrap();

        // Only complete the first one with usage.
        mgr.complete_with_usage(
            &id1,
            "r1".to_string(),
            SubAgentUsage {
                input_tokens: 100,
                output_tokens: 40,
            },
        );
        // id2 is still running (no usage).

        let total = mgr.total_usage();
        assert_eq!(total.input_tokens, 100);
        assert_eq!(total.output_tokens, 40);
    }

    // --- REQ-AGENT-004: Async sub-agent spawning ---

    // rtmx:req REQ-AGENT-004
    #[tokio::test]
    async fn spawn_async_returns_id_and_handle() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(vec![
            StreamEvent::Token("hello".into()),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        let filter = Arc::new(MockSecurityFilter);
        let config = SubAgentConfig::default();

        let result = mgr
            .spawn_async(config, "summarize this".into(), provider, filter)
            .await;
        assert!(result.is_ok());
        let (id, handle) = result.unwrap();
        assert!(!id.is_empty());

        let output = handle.await.unwrap().unwrap();
        assert_eq!(output, "hello");
    }

    // rtmx:req REQ-AGENT-004
    #[tokio::test]
    async fn spawn_async_respects_max_concurrent() {
        let mut mgr = SubAgentManager::new(1);
        let provider = Arc::new(MockLlmProvider::new());
        // Queue two responses (only first will be used).
        provider.queue_response(vec![StreamEvent::Done {
            input_tokens: 0,
            output_tokens: 0,
        }]);
        provider.queue_response(vec![StreamEvent::Done {
            input_tokens: 0,
            output_tokens: 0,
        }]);
        let filter = Arc::new(MockSecurityFilter);

        let _ = mgr
            .spawn_async(
                SubAgentConfig::default(),
                "task 1".into(),
                provider.clone(),
                filter.clone(),
            )
            .await
            .unwrap();

        let result = mgr
            .spawn_async(SubAgentConfig::default(), "task 2".into(), provider, filter)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("concurrency limit"),
            "error should mention concurrency limit: {}",
            err.message
        );
    }

    // rtmx:req REQ-AGENT-004
    #[tokio::test]
    async fn spawn_async_validates_read_only() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        let filter = Arc::new(MockSecurityFilter);
        let config = SubAgentConfig {
            allowed_tools: vec!["read_file".into(), "write_file".into()],
            ..SubAgentConfig::default()
        };

        let result = mgr
            .spawn_async(config, "task".into(), provider, filter)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("write_file"),
            "error should mention write_file: {}",
            err.message
        );
    }

    // rtmx:req REQ-AGENT-004
    #[tokio::test]
    async fn subagent_loop_completes_with_text() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(vec![
            StreamEvent::Token("analysis ".into()),
            StreamEvent::Token("complete".into()),
            StreamEvent::Done {
                input_tokens: 20,
                output_tokens: 10,
            },
        ]);

        let config = SubAgentConfig::default();
        let result = run_subagent_loop(provider, config, "analyze the code".into()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "analysis complete");
    }

    // rtmx:req REQ-AGENT-004
    #[tokio::test]
    async fn subagent_loop_respects_max_iterations() {
        let provider = Arc::new(MockLlmProvider::new());
        // Queue responses that never emit Done -- each iteration will
        // consume one response that has only tokens (no Done).
        for _ in 0..3 {
            provider.queue_response(vec![StreamEvent::Token("partial".into())]);
        }

        let config = SubAgentConfig {
            max_iterations: 3,
            ..SubAgentConfig::default()
        };

        let result = run_subagent_loop(provider, config, "keep going".into()).await;
        // Should succeed with accumulated text from all iterations.
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "partialpartialpartial");
    }

    // --- REQ-AGENT-019: Sub-agent process spawning with async execution ---

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_creates_task() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(vec![
            StreamEvent::Token("working".into()),
            StreamEvent::Done {
                input_tokens: 10,
                output_tokens: 5,
            },
        ]);
        let filter = Arc::new(MockSecurityFilter);

        let (id, _handle) = mgr
            .spawn_async(
                SubAgentConfig::default(),
                "analyze code".into(),
                provider,
                filter,
            )
            .await
            .unwrap();

        // The sub-agent should be registered as Running in the manager.
        assert_eq!(mgr.status(&id), Some(&SubAgentStatus::Running));
        assert_eq!(mgr.active_count(), 1);
    }

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_task_completes_with_text() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(vec![
            StreamEvent::Token("The analysis ".into()),
            StreamEvent::Token("is complete.".into()),
            StreamEvent::Done {
                input_tokens: 25,
                output_tokens: 12,
            },
        ]);
        let filter = Arc::new(MockSecurityFilter);

        let (id, handle) = mgr
            .spawn_async(
                SubAgentConfig::default(),
                "summarize findings".into(),
                provider,
                filter,
            )
            .await
            .unwrap();

        let result = handle.await.unwrap().unwrap();
        assert_eq!(result, "The analysis is complete.");

        // After the handle resolves, integrate result back into manager.
        mgr.complete(&id, result);
        assert_eq!(
            mgr.status(&id),
            Some(&SubAgentStatus::Completed(
                "The analysis is complete.".to_string()
            ))
        );
    }

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_respects_max_iterations() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        // Queue responses with ToolUse on every call (no Done event),
        // forcing iteration until max_iterations is reached.
        let tool_call = aegis_domain::types::ToolCall::ReadFile {
            path: aegis_domain::types::FilePath::new_unchecked("src/main.rs"),
        };
        for _ in 0..2 {
            provider.queue_response(vec![
                StreamEvent::Token("reading...".into()),
                StreamEvent::ToolUse(tool_call.clone()),
            ]);
        }
        let filter = Arc::new(MockSecurityFilter);

        let config = SubAgentConfig {
            max_iterations: 2,
            ..SubAgentConfig::default()
        };

        let (_id, handle) = mgr
            .spawn_async(config, "keep iterating".into(), provider, filter)
            .await
            .unwrap();

        let result = handle.await.unwrap().unwrap();
        // Should have accumulated text from both iterations.
        assert_eq!(result, "reading...reading...");
    }

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_restricts_tools() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(vec![
            StreamEvent::Token("done".into()),
            StreamEvent::Done {
                input_tokens: 5,
                output_tokens: 3,
            },
        ]);

        let config = SubAgentConfig {
            allowed_tools: vec![
                "read_file".to_string(),
                "list_dir".to_string(),
                "grep".to_string(),
            ],
            ..SubAgentConfig::default()
        };

        let _ = run_subagent_loop(provider.clone(), config, "test".into()).await;

        // Verify the tool schemas passed to the LLM contain only read-only tools.
        let captured = provider.captured_tool_schemas.lock().unwrap();
        assert_eq!(captured.len(), 1, "stream() should have been called once");
        let schemas = &captured[0];
        assert_eq!(schemas.len(), 3);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"list_dir"));
        assert!(names.contains(&"grep"));
        // Mutating tools must not be present.
        assert!(!names.contains(&"write_file"));
        assert!(!names.contains(&"run_command"));
    }

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_rejects_mutating_config() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        let filter = Arc::new(MockSecurityFilter);

        // Config with write_file in allowed_tools should be rejected.
        let config = SubAgentConfig {
            allowed_tools: vec!["read_file".to_string(), "write_file".to_string()],
            ..SubAgentConfig::default()
        };

        let result = mgr
            .spawn_async(config, "should fail".into(), provider, filter)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("write_file"),
            "error should mention the mutating tool: {}",
            err.message
        );
        // No agent should have been registered.
        assert_eq!(mgr.active_count(), 0);
    }

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_concurrent_limit() {
        let mut mgr = SubAgentManager::new(2);
        let provider = Arc::new(MockLlmProvider::new());
        // Queue enough responses for all attempts.
        for _ in 0..3 {
            provider.queue_response(vec![StreamEvent::Done {
                input_tokens: 0,
                output_tokens: 0,
            }]);
        }
        let filter = Arc::new(MockSecurityFilter);

        let _ = mgr
            .spawn_async(
                SubAgentConfig::default(),
                "task 1".into(),
                provider.clone(),
                filter.clone(),
            )
            .await
            .unwrap();
        let _ = mgr
            .spawn_async(
                SubAgentConfig::default(),
                "task 2".into(),
                provider.clone(),
                filter.clone(),
            )
            .await
            .unwrap();

        // Third spawn should fail -- max_concurrent is 2.
        let result = mgr
            .spawn_async(SubAgentConfig::default(), "task 3".into(), provider, filter)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("concurrency limit"),
            "error should mention concurrency limit: {}",
            err.message
        );
    }

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_usage_tracked() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(vec![
            StreamEvent::Token("result".into()),
            StreamEvent::Done {
                input_tokens: 150,
                output_tokens: 75,
            },
        ]);
        let filter = Arc::new(MockSecurityFilter);

        let (id, handle) = mgr
            .spawn_async(
                SubAgentConfig::default(),
                "analyze".into(),
                provider,
                filter,
            )
            .await
            .unwrap();

        let result = handle.await.unwrap().unwrap();

        // Record usage via complete_with_usage.
        let usage = SubAgentUsage {
            input_tokens: 150,
            output_tokens: 75,
        };
        mgr.complete_with_usage(&id, result, usage);

        let total = mgr.total_usage();
        assert_eq!(total.input_tokens, 150);
        assert_eq!(total.output_tokens, 75);
    }

    // rtmx:req REQ-AGENT-019
    #[tokio::test]
    async fn spawn_async_error_propagates() {
        let mut mgr = SubAgentManager::new(4);
        let provider = Arc::new(MockLlmProvider::new());
        // Queue a response that contains an error event.
        provider.queue_response(vec![StreamEvent::Error("provider unavailable".to_string())]);
        let filter = Arc::new(MockSecurityFilter);

        let (id, handle) = mgr
            .spawn_async(
                SubAgentConfig::default(),
                "will fail".into(),
                provider,
                filter,
            )
            .await
            .unwrap();

        let result = handle.await.unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            DomainError::ProviderError { message } => {
                assert_eq!(message, "provider unavailable");
            }
            other => panic!("expected ProviderError, got: {:?}", other),
        }

        // Manager should still show Running since we haven't
        // called fail() yet -- the async result carries the error.
        assert_eq!(mgr.status(&id), Some(&SubAgentStatus::Running));

        // Mark it as failed after observing the error.
        mgr.fail(&id, "provider unavailable".to_string());
        assert_eq!(
            mgr.status(&id),
            Some(&SubAgentStatus::Failed("provider unavailable".to_string()))
        );
    }
}
