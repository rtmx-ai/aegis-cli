//! Sub-agent spawning for parallel read-only tasks (REQ-AGENT-004).
//!
//! Sub-agents are lightweight task units that can execute read-only operations
//! in parallel. Each sub-agent has a restricted tool set (read-only by default)
//! and a configurable iteration limit.

use uuid::Uuid;

/// A sub-agent that executes a single task with restricted tools.
#[derive(Debug)]
pub struct SubAgent {
    pub id: String,
    pub task: String,
    pub status: SubAgentStatus,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
    #[test]
    fn default_config_max_iterations_is_10() {
        let config = SubAgentConfig::default();
        assert_eq!(config.max_iterations, 10);
    }

    // @req REQ-AGENT-004
    #[test]
    fn default_config_disallows_nesting() {
        let config = SubAgentConfig::default();
        assert!(!config.allow_nesting);
    }

    // @req REQ-AGENT-004
    #[test]
    fn manager_new_sets_max_concurrent() {
        let mgr = SubAgentManager::new(4);
        assert_eq!(mgr.max_concurrent, 4);
        assert_eq!(mgr.active_count(), 0);
    }

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
    #[test]
    fn status_returns_none_for_unknown_id() {
        let mgr = SubAgentManager::new(4);
        assert_eq!(mgr.status("nonexistent"), None);
    }

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
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

    // @req REQ-AGENT-004
    #[test]
    fn complete_returns_false_for_unknown_id() {
        let mut mgr = SubAgentManager::new(4);
        assert!(!mgr.complete("nonexistent", "result".to_string()));
    }

    // @req REQ-AGENT-004
    #[test]
    fn fail_returns_false_for_unknown_id() {
        let mut mgr = SubAgentManager::new(4);
        assert!(!mgr.fail("nonexistent", "error".to_string()));
    }

    // @req REQ-AGENT-004
    #[test]
    fn spawn_error_display() {
        let err = SpawnError {
            message: "limit reached".to_string(),
        };
        assert_eq!(err.to_string(), "SubAgent spawn error: limit reached");
    }
}
