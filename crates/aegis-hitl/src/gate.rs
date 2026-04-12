//! The HITL approval gate.

use aegis_domain::types::{ToolCall, ToolRisk};

/// Determines whether a tool call requires human approval.
#[derive(Default)]
pub struct HitlGate;

impl HitlGate {
    pub fn requires_approval(&self, tool_call: &ToolCall) -> bool {
        tool_call.risk() == ToolRisk::StateMutating
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
        let gate = HitlGate;
        assert_eq!(gate.requires_approval(&call), requires);
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn all_mutating_tools_require_approval() {
        let gate = HitlGate;
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
}
