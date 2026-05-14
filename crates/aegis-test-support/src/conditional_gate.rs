//! Conditional mock approval gate for per-tool approval policies (REQ-TEST-046).

use aegis_domain::error::DomainError;
use aegis_domain::ports::ApprovalGate;
use aegis_domain::types::{ApprovalDecision, ApprovalResponse, ToolCall, ToolRisk};
use async_trait::async_trait;

/// Matches tool calls by variant name, path prefix, or risk level.
#[derive(Debug, Clone)]
pub enum ToolMatcher {
    /// Matches any tool call.
    AnyTool,
    /// Matches by variant name (e.g., "ReadFile", "WriteFile", "RunCommand").
    ByName(String),
    /// Matches tool calls whose path starts with the given prefix.
    /// Applies to ReadFile, WriteFile, ListDir, and Grep.
    ByPath(String),
    /// Matches tool calls with a specific risk level.
    ByRisk(ToolRisk),
}

impl ToolMatcher {
    /// Returns true if this matcher matches the given tool call.
    pub fn matches(&self, tool_call: &ToolCall) -> bool {
        match self {
            ToolMatcher::AnyTool => true,
            ToolMatcher::ByName(name) => variant_name(tool_call) == name,
            ToolMatcher::ByPath(prefix) => {
                if let Some(path) = extract_path(tool_call) {
                    path.starts_with(prefix.as_str())
                } else {
                    false
                }
            }
            ToolMatcher::ByRisk(risk) => tool_call.risk() == *risk,
        }
    }
}

/// Returns the variant name of a ToolCall as a string.
fn variant_name(tool_call: &ToolCall) -> &'static str {
    match tool_call {
        ToolCall::ReadFile { .. } => "ReadFile",
        ToolCall::WriteFile { .. } => "WriteFile",
        ToolCall::RunCommand { .. } => "RunCommand",
        ToolCall::ListDir { .. } => "ListDir",
        ToolCall::Grep { .. } => "Grep",
        ToolCall::McpTool { .. } => "McpTool",
    }
}

/// Extracts the path string from tool calls that carry one.
fn extract_path(tool_call: &ToolCall) -> Option<String> {
    match tool_call {
        ToolCall::ReadFile { path } => Some(path.to_string()),
        ToolCall::WriteFile { path, .. } => Some(path.to_string()),
        ToolCall::ListDir { path } => Some(path.to_string()),
        ToolCall::Grep { path, .. } => Some(path.to_string()),
        ToolCall::RunCommand { .. } | ToolCall::McpTool { .. } => None,
    }
}

/// A mock approval gate that evaluates rules in order and returns
/// the decision from the first matching rule. Falls back to a default
/// decision when no rule matches.
pub struct ConditionalMockGate {
    rules: Vec<(ToolMatcher, ApprovalDecision)>,
    default: ApprovalDecision,
}

impl ConditionalMockGate {
    /// Create a new conditional gate with the given rules and default decision.
    ///
    /// Rules are evaluated in order; the first matching rule's decision is returned.
    /// If no rule matches, the default decision is returned.
    pub fn new(rules: Vec<(ToolMatcher, ApprovalDecision)>, default: ApprovalDecision) -> Self {
        Self { rules, default }
    }
}

#[async_trait]
impl ApprovalGate for ConditionalMockGate {
    async fn request_approval(
        &self,
        tool_call: &ToolCall,
    ) -> Result<ApprovalResponse, DomainError> {
        for (matcher, decision) in &self.rules {
            if matcher.matches(tool_call) {
                return Ok(ApprovalResponse::simple(*decision));
            }
        }
        Ok(ApprovalResponse::simple(self.default))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;

    // rtmx:req REQ-TEST-046
    #[tokio::test]
    async fn approve_read_file_but_deny_write_file() {
        let gate = ConditionalMockGate::new(
            vec![
                (
                    ToolMatcher::ByName("ReadFile".into()),
                    ApprovalDecision::Approved,
                ),
                (
                    ToolMatcher::ByName("WriteFile".into()),
                    ApprovalDecision::Denied,
                ),
            ],
            ApprovalDecision::Denied,
        );

        let read = ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        };
        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "fn main() {}".into(),
        };

        assert_eq!(
            gate.request_approval(&read).await.unwrap().decision,
            ApprovalDecision::Approved
        );
        assert_eq!(
            gate.request_approval(&write).await.unwrap().decision,
            ApprovalDecision::Denied
        );
    }

    // rtmx:req REQ-TEST-046
    #[tokio::test]
    async fn path_based_matching() {
        let gate = ConditionalMockGate::new(
            vec![
                (
                    ToolMatcher::ByPath("src/".into()),
                    ApprovalDecision::Approved,
                ),
                (
                    ToolMatcher::ByPath("/etc/".into()),
                    ApprovalDecision::Denied,
                ),
            ],
            ApprovalDecision::Denied,
        );

        let write_src = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/lib.rs"),
            content: String::new(),
        };
        let write_etc = ToolCall::WriteFile {
            path: FilePath::new_unchecked("/etc/passwd"),
            content: String::new(),
        };

        assert_eq!(
            gate.request_approval(&write_src).await.unwrap().decision,
            ApprovalDecision::Approved
        );
        assert_eq!(
            gate.request_approval(&write_etc).await.unwrap().decision,
            ApprovalDecision::Denied
        );
    }

    // rtmx:req REQ-TEST-046
    #[tokio::test]
    async fn default_deny_when_no_rules_match() {
        let gate = ConditionalMockGate::new(
            vec![(
                ToolMatcher::ByName("ReadFile".into()),
                ApprovalDecision::Approved,
            )],
            ApprovalDecision::Denied,
        );

        let cmd = ToolCall::RunCommand {
            command: "rm -rf /".into(),
            timeout_secs: 10,
        };

        assert_eq!(
            gate.request_approval(&cmd).await.unwrap().decision,
            ApprovalDecision::Denied
        );
    }

    // rtmx:req REQ-TEST-046
    #[tokio::test]
    async fn first_matching_rule_wins() {
        let gate = ConditionalMockGate::new(
            vec![
                (
                    ToolMatcher::ByName("WriteFile".into()),
                    ApprovalDecision::Approved,
                ),
                (
                    ToolMatcher::ByRisk(ToolRisk::StateMutating),
                    ApprovalDecision::Denied,
                ),
            ],
            ApprovalDecision::Denied,
        );

        // WriteFile is StateMutating, but the ByName rule comes first.
        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked("test.txt"),
            content: "hello".into(),
        };

        assert_eq!(
            gate.request_approval(&write).await.unwrap().decision,
            ApprovalDecision::Approved
        );

        // RunCommand is also StateMutating, but no ByName("RunCommand") rule,
        // so the ByRisk rule fires.
        let cmd = ToolCall::RunCommand {
            command: "echo hi".into(),
            timeout_secs: 5,
        };

        assert_eq!(
            gate.request_approval(&cmd).await.unwrap().decision,
            ApprovalDecision::Denied
        );
    }

    // rtmx:req REQ-TEST-046
    #[tokio::test]
    async fn any_tool_matches_everything() {
        let gate = ConditionalMockGate::new(
            vec![(ToolMatcher::AnyTool, ApprovalDecision::Approved)],
            ApprovalDecision::Denied,
        );

        let calls: Vec<ToolCall> = vec![
            ToolCall::ReadFile {
                path: FilePath::new_unchecked("a.rs"),
            },
            ToolCall::WriteFile {
                path: FilePath::new_unchecked("b.rs"),
                content: String::new(),
            },
            ToolCall::RunCommand {
                command: "ls".into(),
                timeout_secs: 5,
            },
            ToolCall::ListDir {
                path: FilePath::new_unchecked("."),
            },
            ToolCall::Grep {
                pattern: "TODO".into(),
                path: FilePath::new_unchecked("src"),
            },
        ];

        for call in &calls {
            assert_eq!(
                gate.request_approval(call).await.unwrap().decision,
                ApprovalDecision::Approved,
                "AnyTool should match {:?}",
                call
            );
        }
    }
}
