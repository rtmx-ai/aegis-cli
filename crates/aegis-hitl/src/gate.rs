//! The HITL approval gate.

use aegis_domain::types::{ToolCall, ToolRisk};
use std::collections::HashSet;

/// Determines whether a tool call requires human approval.
///
/// Maintains a session-scoped allow-list of tool names that bypass
/// the approval gate (REQ-HITL-019).
#[derive(Default)]
pub struct HitlGate {
    /// Tool names granted "always allow" for this session.
    always_allowed: HashSet<String>,
}

impl HitlGate {
    /// Returns true if the tool call requires human approval.
    ///
    /// A tool is exempt from approval if it is read-only (non-mutating)
    /// or if it has been granted always-allow for this session.
    pub fn requires_approval(&self, tool_call: &ToolCall) -> bool {
        if tool_call.risk() != ToolRisk::StateMutating {
            return false;
        }
        !self.always_allowed.contains(tool_call.tool_name())
    }

    /// Grant always-allow for a tool name for the remainder of this session.
    pub fn grant_always_allow(&mut self, tool_name: &str) {
        self.always_allowed.insert(tool_name.to_string());
    }

    /// Revoke a previously granted always-allow.
    pub fn revoke_always_allow(&mut self, tool_name: &str) {
        self.always_allowed.remove(tool_name);
    }

    /// Returns true if the tool name has an active always-allow grant.
    pub fn is_always_allowed(&self, tool_name: &str) -> bool {
        self.always_allowed.contains(tool_name)
    }

    /// Clear all always-allow grants (e.g., on session end).
    pub fn clear_always_allowed(&mut self) {
        self.always_allowed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;
    use rstest::*;

    // rtmx:req REQ-HITL-001
    #[rstest]
    #[case(ToolCall::WriteFile { path: FilePath::new_unchecked("src/main.rs"), content: String::new() }, true)]
    #[case(ToolCall::ReadFile { path: FilePath::new_unchecked("src/main.rs") }, false)]
    #[case(ToolCall::RunCommand { command: "npm test".into(), timeout_secs: 60 }, true)]
    #[case(ToolCall::ListDir { path: FilePath::new_unchecked(".") }, false)]
    #[case(ToolCall::Grep { pattern: "TODO".into(), path: FilePath::new_unchecked("src") }, false)]
    fn hitl_gate_classifies_risk(#[case] call: ToolCall, #[case] requires: bool) {
        let gate = HitlGate::default();
        assert_eq!(gate.requires_approval(&call), requires);
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn all_mutating_tools_require_approval() {
        let gate = HitlGate::default();
        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked("any.txt"),
            content: "data".into(),
        };
        let exec = ToolCall::RunCommand {
            command: "echo hello".into(),
            timeout_secs: 10,
        };
        assert!(gate.requires_approval(&write));
        assert!(gate.requires_approval(&exec));
    }

    // rtmx:req REQ-HITL-019
    #[test]
    fn test_always_allow_bypasses_gate() {
        let mut gate = HitlGate::default();
        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "code".into(),
        };

        // Before grant: requires approval
        assert!(gate.requires_approval(&write));

        // Grant always-allow for write_file
        gate.grant_always_allow("write_file");

        // After grant: bypasses approval
        assert!(!gate.requires_approval(&write));
        assert!(gate.is_always_allowed("write_file"));
    }

    // rtmx:req REQ-HITL-019
    #[test]
    fn test_always_allow_is_tool_specific() {
        let mut gate = HitlGate::default();
        gate.grant_always_allow("write_file");

        // write_file bypassed
        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked("a.rs"),
            content: "x".into(),
        };
        assert!(!gate.requires_approval(&write));

        // run_command still requires approval
        let exec = ToolCall::RunCommand {
            command: "rm -rf /".into(),
            timeout_secs: 10,
        };
        assert!(gate.requires_approval(&exec));
    }

    // rtmx:req REQ-HITL-019
    #[test]
    fn test_always_allow_revoke() {
        let mut gate = HitlGate::default();
        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked("a.rs"),
            content: "x".into(),
        };

        gate.grant_always_allow("write_file");
        assert!(!gate.requires_approval(&write));

        gate.revoke_always_allow("write_file");
        assert!(gate.requires_approval(&write));
        assert!(!gate.is_always_allowed("write_file"));
    }

    // rtmx:req REQ-HITL-019
    #[test]
    fn test_always_allow_clear_all() {
        let mut gate = HitlGate::default();
        gate.grant_always_allow("write_file");
        gate.grant_always_allow("run_command");

        gate.clear_always_allowed();

        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked("a.rs"),
            content: "x".into(),
        };
        let exec = ToolCall::RunCommand {
            command: "echo".into(),
            timeout_secs: 10,
        };
        assert!(gate.requires_approval(&write));
        assert!(gate.requires_approval(&exec));
    }

    // rtmx:req REQ-HITL-019
    #[test]
    fn test_always_allow_read_only_still_no_approval() {
        let mut gate = HitlGate::default();
        // Read-only tools never need approval regardless of allow-list
        let read = ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        };
        assert!(!gate.requires_approval(&read));

        // Granting always-allow for read_file is a no-op but doesn't error
        gate.grant_always_allow("read_file");
        assert!(!gate.requires_approval(&read));
    }

    // rtmx:req REQ-HITL-019
    #[test]
    fn test_always_allow_mcp_tool() {
        let mut gate = HitlGate::default();
        let mcp = ToolCall::McpTool {
            qualified_name: "server/my_tool".into(),
            arguments: "{}".into(),
        };
        assert!(gate.requires_approval(&mcp));

        gate.grant_always_allow("server/my_tool");
        assert!(!gate.requires_approval(&mcp));
    }
}
