//! Dispatch manager for parallel worktree agents (REQ-AGENT-036).
//!
//! This module provides the state machine and tracking layer that manages
//! dispatching workstreams to isolated git worktrees. It handles registration,
//! status supervision (REQ-AGENT-037), merge readiness (REQ-AGENT-038), and
//! plan display formatting (REQ-AGENT-040).
//!
//! The dispatch manager is a pure state tracker -- actual git worktree
//! creation and agent execution are handled by higher-level orchestration
//! code that drives this state machine.

use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for dispatching a workstream to a worktree.
#[derive(Debug, Clone)]
pub struct DispatchConfig {
    /// Human-readable name for this workstream.
    pub workstream_name: String,
    /// Requirement IDs this workstream addresses.
    pub requirements: Vec<String>,
    /// Instruction prompt for the agent.
    pub prompt: String,
    /// Path where the worktree will be created.
    pub worktree_path: PathBuf,
    /// Git branch name for this worktree.
    pub branch_name: String,
}

/// Status of a dispatched workstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchStatus {
    /// Registered but not yet started.
    Pending,
    /// Agent is actively running in the worktree.
    Running,
    /// Agent completed successfully.
    Completed {
        /// Number of tests added by this workstream.
        tests_added: usize,
    },
    /// Agent failed with an error.
    Failed {
        /// Description of the failure.
        error: String,
    },
    /// Workstream has been merged into the target branch.
    Merged,
}

/// Internal tracking entry for a dispatched workstream.
struct DispatchEntry {
    #[allow(dead_code)]
    config: DispatchConfig,
    status: DispatchStatus,
}

/// REQ-AGENT-036: Dispatch manager for parallel worktree agents.
///
/// Tracks the lifecycle of workstreams dispatched to isolated git
/// worktrees. Provides supervision (REQ-AGENT-037), merge readiness
/// checks (REQ-AGENT-038), and display formatting (REQ-AGENT-040).
pub struct DispatchManager {
    dispatches: HashMap<String, DispatchEntry>,
    max_concurrent: usize,
    repo_root: PathBuf,
}

/// Summary counts of dispatch statuses (REQ-AGENT-037).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchSummary {
    /// Number of workstreams not yet started.
    pub pending: usize,
    /// Number of workstreams actively running.
    pub running: usize,
    /// Number of workstreams that completed successfully.
    pub completed: usize,
    /// Number of workstreams that failed.
    pub failed: usize,
    /// Number of workstreams that have been merged.
    pub merged: usize,
}

impl DispatchManager {
    /// Create a new dispatch manager.
    ///
    /// # Arguments
    /// * `repo_root` - Path to the repository root.
    /// * `max_concurrent` - Maximum number of concurrently running dispatches.
    pub fn new(repo_root: PathBuf, max_concurrent: usize) -> Self {
        Self {
            dispatches: HashMap::new(),
            max_concurrent,
            repo_root,
        }
    }

    /// REQ-AGENT-036: Create a worktree directory structure for a dispatch.
    ///
    /// This creates the directory at `config.worktree_path`. In production,
    /// the caller would also run `git worktree add`; this method handles
    /// only the filesystem preparation.
    pub fn prepare_worktree(&self, config: &DispatchConfig) -> std::io::Result<PathBuf> {
        let path = if config.worktree_path.is_relative() {
            self.repo_root.join(&config.worktree_path)
        } else {
            config.worktree_path.clone()
        };
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Register a dispatch for tracking. The dispatch starts in `Pending`
    /// status and does not execute until explicitly transitioned to
    /// `Running`.
    pub fn register(&mut self, config: DispatchConfig) {
        let name = config.workstream_name.clone();
        self.dispatches.insert(
            name,
            DispatchEntry {
                config,
                status: DispatchStatus::Pending,
            },
        );
    }

    /// REQ-AGENT-037: Get the status of all dispatches.
    ///
    /// Returns a list of (workstream_name, status) pairs sorted by name
    /// for deterministic output.
    pub fn status(&self) -> Vec<(String, DispatchStatus)> {
        let mut result: Vec<(String, DispatchStatus)> = self
            .dispatches
            .iter()
            .map(|(name, entry)| (name.clone(), entry.status.clone()))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// REQ-AGENT-037: Get summary counts by status.
    pub fn summary(&self) -> DispatchSummary {
        let mut s = DispatchSummary {
            pending: 0,
            running: 0,
            completed: 0,
            failed: 0,
            merged: 0,
        };
        for entry in self.dispatches.values() {
            match entry.status {
                DispatchStatus::Pending => s.pending += 1,
                DispatchStatus::Running => s.running += 1,
                DispatchStatus::Completed { .. } => s.completed += 1,
                DispatchStatus::Failed { .. } => s.failed += 1,
                DispatchStatus::Merged => s.merged += 1,
            }
        }
        s
    }

    /// Update the status of a dispatch by name. If the name is not found,
    /// this is a no-op.
    pub fn update_status(&mut self, name: &str, status: DispatchStatus) {
        if let Some(entry) = self.dispatches.get_mut(name) {
            entry.status = status;
        }
    }

    /// REQ-AGENT-038: Get names of dispatches ready to merge.
    ///
    /// Returns workstream names that have `Completed` status, sorted
    /// alphabetically for deterministic ordering.
    pub fn ready_to_merge(&self) -> Vec<String> {
        let mut result: Vec<String> = self
            .dispatches
            .iter()
            .filter(|(_, entry)| matches!(entry.status, DispatchStatus::Completed { .. }))
            .map(|(name, _)| name.clone())
            .collect();
        result.sort();
        result
    }

    /// REQ-AGENT-038: Record a successful merge.
    ///
    /// Transitions the dispatch from `Completed` to `Merged`. If the name
    /// is not found or the dispatch is not in `Completed` status, this is
    /// a no-op.
    pub fn mark_merged(&mut self, name: &str) {
        if let Some(entry) = self.dispatches.get_mut(name)
            && matches!(entry.status, DispatchStatus::Completed { .. })
        {
            entry.status = DispatchStatus::Merged;
        }
    }

    /// Get the count of active (Running) dispatches.
    pub fn active_count(&self) -> usize {
        self.dispatches
            .values()
            .filter(|e| e.status == DispatchStatus::Running)
            .count()
    }

    /// Check whether more dispatches can be started without exceeding
    /// the concurrency limit.
    pub fn can_dispatch(&self) -> bool {
        self.active_count() < self.max_concurrent
    }
}

/// REQ-AGENT-040: Format a plan for display.
///
/// Produces a human-readable table of workstream statuses suitable for
/// terminal output. Includes a wave header and per-workstream status
/// lines.
pub fn format_plan_display(dispatches: &[(String, DispatchStatus)], wave_index: usize) -> String {
    let mut output = String::new();
    output.push_str(&format!("=== Wave {} ===\n", wave_index));
    output.push_str(&format!("{:<30} {}\n", "Workstream", "Status"));
    output.push_str(&"-".repeat(50));
    output.push('\n');

    for (name, status) in dispatches {
        let status_str = match status {
            DispatchStatus::Pending => "Pending".to_string(),
            DispatchStatus::Running => "Running".to_string(),
            DispatchStatus::Completed { tests_added } => {
                format!("Completed ({} tests)", tests_added)
            }
            DispatchStatus::Failed { error } => {
                format!("Failed: {}", error)
            }
            DispatchStatus::Merged => "Merged".to_string(),
        };
        output.push_str(&format!("{:<30} {}\n", name, status_str));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_config(name: &str) -> DispatchConfig {
        DispatchConfig {
            workstream_name: name.to_string(),
            requirements: vec!["REQ-AGENT-036".to_string()],
            prompt: format!("Implement {}", name),
            worktree_path: PathBuf::from(format!(".worktrees/{}", name)),
            branch_name: format!("agent/{}", name),
        }
    }

    fn test_manager() -> DispatchManager {
        DispatchManager::new(PathBuf::from("/tmp/repo"), 3)
    }

    // --- REQ-AGENT-036: Dispatch manager basics ---

    // rtmx:req REQ-AGENT-036
    #[test]
    fn dispatch_manager_starts_empty() {
        let mgr = test_manager();
        assert!(mgr.status().is_empty());
        assert_eq!(mgr.active_count(), 0);
        assert!(mgr.can_dispatch());
    }

    // rtmx:req REQ-AGENT-036
    #[test]
    fn register_adds_dispatch() {
        let mut mgr = test_manager();
        mgr.register(test_config("feature-a"));

        let statuses = mgr.status();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0, "feature-a");
        assert_eq!(statuses[0].1, DispatchStatus::Pending);
    }

    // rtmx:req REQ-AGENT-036
    #[test]
    fn prepare_worktree_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = DispatchManager::new(dir.path().to_path_buf(), 3);

        let config = DispatchConfig {
            workstream_name: "ws-1".to_string(),
            requirements: vec![],
            prompt: String::new(),
            worktree_path: PathBuf::from("worktrees/ws-1"),
            branch_name: "agent/ws-1".to_string(),
        };

        let result = mgr.prepare_worktree(&config);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.is_dir());
    }

    // rtmx:req REQ-AGENT-036
    #[test]
    fn can_dispatch_respects_max_concurrent() {
        let mut mgr = DispatchManager::new(PathBuf::from("/tmp"), 2);

        mgr.register(test_config("a"));
        mgr.register(test_config("b"));
        mgr.register(test_config("c"));

        // All pending -- none running.
        assert!(mgr.can_dispatch());

        mgr.update_status("a", DispatchStatus::Running);
        assert!(mgr.can_dispatch());

        mgr.update_status("b", DispatchStatus::Running);
        assert!(!mgr.can_dispatch());

        // Complete one -- should free a slot.
        mgr.update_status("a", DispatchStatus::Completed { tests_added: 3 });
        assert!(mgr.can_dispatch());
    }

    // rtmx:req REQ-AGENT-036
    #[test]
    fn active_count_tracks_running() {
        let mut mgr = test_manager();
        mgr.register(test_config("a"));
        mgr.register(test_config("b"));
        mgr.register(test_config("c"));

        assert_eq!(mgr.active_count(), 0);

        mgr.update_status("a", DispatchStatus::Running);
        assert_eq!(mgr.active_count(), 1);

        mgr.update_status("b", DispatchStatus::Running);
        assert_eq!(mgr.active_count(), 2);

        mgr.update_status("a", DispatchStatus::Completed { tests_added: 1 });
        assert_eq!(mgr.active_count(), 1);

        mgr.update_status(
            "b",
            DispatchStatus::Failed {
                error: "oops".to_string(),
            },
        );
        assert_eq!(mgr.active_count(), 0);
    }

    // --- REQ-AGENT-037: Supervision ---

    // rtmx:req REQ-AGENT-037
    #[test]
    fn status_returns_all_dispatches() {
        let mut mgr = test_manager();
        mgr.register(test_config("alpha"));
        mgr.register(test_config("beta"));
        mgr.register(test_config("gamma"));

        let statuses = mgr.status();
        assert_eq!(statuses.len(), 3);

        let names: Vec<&str> = statuses.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(names.contains(&"gamma"));
    }

    // rtmx:req REQ-AGENT-037
    #[test]
    fn summary_counts_by_status() {
        let mut mgr = test_manager();
        mgr.register(test_config("a"));
        mgr.register(test_config("b"));
        mgr.register(test_config("c"));
        mgr.register(test_config("d"));
        mgr.register(test_config("e"));

        mgr.update_status("a", DispatchStatus::Running);
        mgr.update_status("b", DispatchStatus::Running);
        mgr.update_status("c", DispatchStatus::Completed { tests_added: 5 });
        mgr.update_status(
            "d",
            DispatchStatus::Failed {
                error: "timeout".to_string(),
            },
        );
        // "e" stays Pending

        let s = mgr.summary();
        assert_eq!(s.pending, 1);
        assert_eq!(s.running, 2);
        assert_eq!(s.completed, 1);
        assert_eq!(s.failed, 1);
        assert_eq!(s.merged, 0);
    }

    // rtmx:req REQ-AGENT-037
    #[test]
    fn update_status_changes_state() {
        let mut mgr = test_manager();
        mgr.register(test_config("ws"));

        // Pending -> Running
        mgr.update_status("ws", DispatchStatus::Running);
        let statuses = mgr.status();
        assert_eq!(statuses[0].1, DispatchStatus::Running);

        // Running -> Completed
        mgr.update_status("ws", DispatchStatus::Completed { tests_added: 7 });
        let statuses = mgr.status();
        assert_eq!(statuses[0].1, DispatchStatus::Completed { tests_added: 7 });
    }

    // --- REQ-AGENT-038: Safe merge ---

    // rtmx:req REQ-AGENT-038
    #[test]
    fn ready_to_merge_returns_completed() {
        let mut mgr = test_manager();
        mgr.register(test_config("a"));
        mgr.register(test_config("b"));

        mgr.update_status("a", DispatchStatus::Completed { tests_added: 2 });
        mgr.update_status("b", DispatchStatus::Completed { tests_added: 3 });

        let ready = mgr.ready_to_merge();
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&"a".to_string()));
        assert!(ready.contains(&"b".to_string()));
    }

    // rtmx:req REQ-AGENT-038
    #[test]
    fn ready_to_merge_excludes_running() {
        let mut mgr = test_manager();
        mgr.register(test_config("a"));
        mgr.register(test_config("b"));

        mgr.update_status("a", DispatchStatus::Running);
        mgr.update_status("b", DispatchStatus::Completed { tests_added: 1 });

        let ready = mgr.ready_to_merge();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "b");
    }

    // rtmx:req REQ-AGENT-038
    #[test]
    fn mark_merged_updates_status() {
        let mut mgr = test_manager();
        mgr.register(test_config("ws"));
        mgr.update_status("ws", DispatchStatus::Completed { tests_added: 4 });

        mgr.mark_merged("ws");

        let statuses = mgr.status();
        assert_eq!(statuses[0].1, DispatchStatus::Merged);

        // Should no longer be in ready_to_merge.
        assert!(mgr.ready_to_merge().is_empty());
    }

    // rtmx:req REQ-AGENT-038
    #[test]
    fn mark_merged_unknown_is_noop() {
        let mut mgr = test_manager();
        // Should not panic.
        mgr.mark_merged("nonexistent");
        assert!(mgr.status().is_empty());
    }

    // --- REQ-AGENT-040: Plan display ---

    // rtmx:req REQ-AGENT-040
    #[test]
    fn format_plan_display_includes_names() {
        let dispatches = vec![
            ("feature-auth".to_string(), DispatchStatus::Running),
            (
                "feature-tui".to_string(),
                DispatchStatus::Completed { tests_added: 3 },
            ),
        ];

        let output = format_plan_display(&dispatches, 1);
        assert!(
            output.contains("feature-auth"),
            "output should contain feature-auth: {}",
            output
        );
        assert!(
            output.contains("feature-tui"),
            "output should contain feature-tui: {}",
            output
        );
    }

    // rtmx:req REQ-AGENT-040
    #[test]
    fn format_plan_display_shows_status() {
        let dispatches = vec![
            ("a".to_string(), DispatchStatus::Running),
            (
                "b".to_string(),
                DispatchStatus::Completed { tests_added: 5 },
            ),
            (
                "c".to_string(),
                DispatchStatus::Failed {
                    error: "build error".to_string(),
                },
            ),
        ];

        let output = format_plan_display(&dispatches, 0);
        assert!(
            output.contains("Running"),
            "should show Running: {}",
            output
        );
        assert!(
            output.contains("Completed"),
            "should show Completed: {}",
            output
        );
        assert!(output.contains("Failed"), "should show Failed: {}", output);
        assert!(
            output.contains("build error"),
            "should show error detail: {}",
            output
        );
    }

    // rtmx:req REQ-AGENT-040
    #[test]
    fn format_plan_display_with_wave() {
        let dispatches = vec![("ws-1".to_string(), DispatchStatus::Pending)];

        let output = format_plan_display(&dispatches, 2);
        assert!(
            output.contains("Wave 2"),
            "should show wave number: {}",
            output
        );
    }
}
