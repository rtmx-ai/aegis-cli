//! Batch approval for homogeneous tool call sequences.
//!
//! Reduces approval fatigue by letting the user approve a class of tool calls
//! in one action (e.g., "all read_file in src/"). The `BatchApprovalManager`
//! sits between the adversary review and the HITL gate: if a tool call is
//! covered by an active batch rule, the HITL gate is bypassed.
//!
//! rtmx:req REQ-HITL-004

use aegis_domain::types::ToolCall;
use std::time::{Duration, Instant};

/// How long a batch approval rule stays active before auto-expiring.
const BATCH_EXPIRY: Duration = Duration::from_secs(600); // 10 minutes

/// A batch approval rule that covers multiple future tool calls.
#[derive(Debug, Clone)]
pub struct BatchApproval {
    /// Tool name pattern (e.g., "read_file", "write_file").
    pub tool_name: String,
    /// Optional path prefix filter (e.g., "src/").
    pub path_prefix: Option<String>,
    /// How many remaining approvals before this batch expires.
    /// `None` means unlimited until time expiry.
    pub remaining: Option<usize>,
    /// Created timestamp for expiry tracking.
    pub created_at: Instant,
}

/// Manages a set of batch approval rules.
///
/// Tool calls that match an active rule are pre-approved, bypassing the
/// interactive HITL gate.
pub struct BatchApprovalManager {
    rules: Vec<BatchApproval>,
}

impl BatchApprovalManager {
    /// Create an empty manager with no active rules.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a batch approval rule.
    pub fn add_rule(&mut self, rule: BatchApproval) {
        self.rules.push(rule);
    }

    /// Check if a tool call is covered by an existing batch rule.
    ///
    /// If covered, decrement the remaining count (if set) and return `true`.
    /// Expired rules are skipped but not pruned (call [`prune_expired`] for
    /// that).
    pub fn check_and_consume(&mut self, tool_call: &ToolCall) -> bool {
        let name = tool_name(tool_call);
        let path = tool_path(tool_call);

        for rule in &mut self.rules {
            // Skip expired rules.
            if rule.created_at.elapsed() >= BATCH_EXPIRY {
                continue;
            }
            // Skip exhausted rules.
            if rule.remaining == Some(0) {
                continue;
            }
            // Match tool name.
            if rule.tool_name != name {
                continue;
            }
            // Match path prefix if configured.
            if let Some(ref prefix) = rule.path_prefix {
                match path {
                    Some(p) if p.starts_with(prefix.as_str()) => {}
                    _ => continue,
                }
            }
            // Rule matches -- consume one use if remaining is set.
            if let Some(ref mut rem) = rule.remaining {
                *rem = rem.saturating_sub(1);
            }
            return true;
        }
        false
    }

    /// Remove expired rules (older than 10 minutes) and exhausted rules.
    pub fn prune_expired(&mut self) {
        self.prune_older_than(BATCH_EXPIRY);
    }

    /// Remove rules older than `max_age` and exhausted rules.
    pub fn prune_older_than(&mut self, max_age: Duration) {
        self.rules
            .retain(|r| r.created_at.elapsed() < max_age && r.remaining != Some(0));
    }
}

impl Default for BatchApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the canonical tool name from a ToolCall variant.
fn tool_name(tool_call: &ToolCall) -> &str {
    tool_call.tool_name()
}

/// Extract the file path from a ToolCall, if applicable.
fn tool_path(tool_call: &ToolCall) -> Option<&str> {
    match tool_call {
        ToolCall::ReadFile { path } => Some(path.as_path().to_str().unwrap_or("")),
        ToolCall::WriteFile { path, .. } => Some(path.as_path().to_str().unwrap_or("")),
        ToolCall::ListDir { path } => Some(path.as_path().to_str().unwrap_or("")),
        ToolCall::Grep { path, .. } => Some(path.as_path().to_str().unwrap_or("")),
        ToolCall::RunCommand { .. } | ToolCall::McpTool { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;

    // rtmx:req REQ-HITL-004
    #[test]
    fn test_batch_approval_matches_tool_name() {
        let mut mgr = BatchApprovalManager::new();
        mgr.add_rule(BatchApproval {
            tool_name: "read_file".to_string(),
            path_prefix: None,
            remaining: None,
            created_at: Instant::now(),
        });

        let call = ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        };
        assert!(mgr.check_and_consume(&call));

        // Different tool name should not match.
        let write_call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "x".into(),
        };
        assert!(!mgr.check_and_consume(&write_call));
    }

    // rtmx:req REQ-HITL-004
    #[test]
    fn test_batch_approval_respects_path_prefix() {
        let mut mgr = BatchApprovalManager::new();
        mgr.add_rule(BatchApproval {
            tool_name: "write_file".to_string(),
            path_prefix: Some("src/".to_string()),
            remaining: None,
            created_at: Instant::now(),
        });

        // Matches prefix.
        let call_in_src = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "code".into(),
        };
        assert!(mgr.check_and_consume(&call_in_src));

        // Does not match prefix.
        let call_in_tests = ToolCall::WriteFile {
            path: FilePath::new_unchecked("tests/foo.rs"),
            content: "test".into(),
        };
        assert!(!mgr.check_and_consume(&call_in_tests));
    }

    // rtmx:req REQ-HITL-004
    #[test]
    fn test_batch_approval_decrements_remaining() {
        let mut mgr = BatchApprovalManager::new();
        mgr.add_rule(BatchApproval {
            tool_name: "read_file".to_string(),
            path_prefix: None,
            remaining: Some(3),
            created_at: Instant::now(),
        });

        let call = ToolCall::ReadFile {
            path: FilePath::new_unchecked("a.rs"),
        };

        assert!(mgr.check_and_consume(&call));
        assert!(mgr.check_and_consume(&call));
        assert!(mgr.check_and_consume(&call));
        // 4th should fail -- remaining exhausted.
        assert!(!mgr.check_and_consume(&call));
    }

    // rtmx:req REQ-HITL-004
    #[test]
    fn test_batch_approval_prune_expired() {
        let mut mgr = BatchApprovalManager::new();

        // Add a rule created right now.
        mgr.add_rule(BatchApproval {
            tool_name: "read_file".to_string(),
            path_prefix: None,
            remaining: None,
            created_at: Instant::now(),
        });

        // Rule is fresh -- prune_older_than(ZERO) treats everything as
        // expired, removing it. This avoids Instant subtraction which
        // panics on Windows when the result would be negative.
        assert_eq!(mgr.rules.len(), 1);
        mgr.prune_older_than(Duration::ZERO);
        assert!(mgr.rules.is_empty(), "all rules should be pruned");
    }

    // rtmx:req REQ-HITL-004
    #[test]
    fn test_batch_approval_prune_removes_exhausted() {
        let mut mgr = BatchApprovalManager::new();
        mgr.add_rule(BatchApproval {
            tool_name: "read_file".to_string(),
            path_prefix: None,
            remaining: Some(1),
            created_at: Instant::now(),
        });

        let call = ToolCall::ReadFile {
            path: FilePath::new_unchecked("a.rs"),
        };
        assert!(mgr.check_and_consume(&call));
        // Now remaining is 0.

        mgr.prune_expired();
        assert!(mgr.rules.is_empty());
    }

    // rtmx:req REQ-HITL-004
    #[test]
    fn test_batch_default_is_empty() {
        let mgr = BatchApprovalManager::default();
        assert!(mgr.rules.is_empty());
    }
}
