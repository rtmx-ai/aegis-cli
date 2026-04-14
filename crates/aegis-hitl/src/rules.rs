//! Permission rules engine for graduated trust.
//!
//! Evaluates tool calls against configurable allow/deny rules, persistent
//! grants, and a trust level to produce an authorization decision.
//! rtmx:req REQ-HITL-002

use aegis_domain::types::ToolCall;
use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};

use super::grants::PermissionGrant;

/// Trust level for graduated permissions (REQ-HITL-002).
///
/// Controls the default behavior when no explicit rule matches a tool call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    /// Ask for approval on every mutating tool call (default).
    #[default]
    Ask,
    /// Auto-approve file writes, still ask for commands.
    AcceptEdits,
    /// Auto-approve everything (dangerous, requires explicit opt-in).
    FullAuto,
}

/// Effect of a permission rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleEffect {
    Allow,
    Deny,
}

/// A single permission rule matching tool calls.
///
/// Rules are evaluated in order; the first matching rule wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Tool name to match ("read_file", "write_file", "run_command",
    /// "list_dir", "grep", or "*" for all tools).
    pub tool: String,
    /// Optional glob pattern for path argument (e.g., "src/**/*.rs").
    pub path_pattern: Option<String>,
    /// Allow or deny matching calls.
    pub effect: RuleEffect,
}

/// Result of evaluating a tool call against rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDecision {
    /// Rule explicitly allows -- skip HITL approval.
    Allow,
    /// Rule explicitly denies -- reject without asking.
    Deny,
    /// No matching rule -- fall back to trust level / risk-based gate.
    AskUser,
}

/// The rules engine evaluates tool calls against configured rules and
/// grants to produce an authorization decision.
pub struct RulesEngine {
    trust_level: TrustLevel,
    rules: Vec<PermissionRule>,
    grants: Vec<PermissionGrant>,
}

impl RulesEngine {
    /// Create a new rules engine with the given trust level and static rules.
    pub fn new(trust_level: TrustLevel, rules: Vec<PermissionRule>) -> Self {
        Self {
            trust_level,
            rules,
            grants: Vec::new(),
        }
    }

    /// Add persistent grants to the engine.
    pub fn with_grants(mut self, grants: Vec<PermissionGrant>) -> Self {
        self.grants = grants;
        self
    }

    /// Evaluate a tool call against grants, rules, and trust level.
    ///
    /// Evaluation order:
    /// 1. Active grants (newest first, first match wins) -> Allow
    /// 2. Static rules (first match wins) -> Allow or Deny
    /// 3. Trust level fallback -> Allow, Deny, or AskUser
    pub fn evaluate(&self, tool_call: &ToolCall) -> RuleDecision {
        // 1. Check grants (newest first).
        let now = chrono::Utc::now();
        for grant in self.grants.iter().rev() {
            if grant.expires_at > now && matches_rule(&grant.rule, tool_call) {
                return RuleDecision::Allow;
            }
        }

        // 2. Check static rules (first match wins).
        for rule in &self.rules {
            if matches_rule(rule, tool_call) {
                return match rule.effect {
                    RuleEffect::Allow => RuleDecision::Allow,
                    RuleEffect::Deny => RuleDecision::Deny,
                };
            }
        }

        // 3. Trust level fallback.
        match self.trust_level {
            TrustLevel::Ask => RuleDecision::AskUser,
            TrustLevel::AcceptEdits => match tool_call {
                ToolCall::WriteFile { .. } => RuleDecision::Allow,
                ToolCall::RunCommand { .. } => RuleDecision::AskUser,
                _ => RuleDecision::Allow,
            },
            TrustLevel::FullAuto => RuleDecision::Allow,
        }
    }

    /// Add a grant to the engine.
    pub fn add_grant(&mut self, grant: PermissionGrant) {
        self.grants.push(grant);
    }

    /// Remove grants that have expired.
    pub fn prune_expired(&mut self) {
        let now = chrono::Utc::now();
        self.grants.retain(|g| g.expires_at > now);
    }
}

/// Extract the canonical tool name from a ToolCall variant.
fn tool_name(tool_call: &ToolCall) -> &str {
    match tool_call {
        ToolCall::ReadFile { .. } => "read_file",
        ToolCall::WriteFile { .. } => "write_file",
        ToolCall::RunCommand { .. } => "run_command",
        ToolCall::ListDir { .. } => "list_dir",
        ToolCall::Grep { .. } => "grep",
        ToolCall::McpTool { qualified_name, .. } => qualified_name.as_str(),
    }
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

/// Check if a rule's tool field matches the tool call.
fn matches_tool(rule: &PermissionRule, tool_call: &ToolCall) -> bool {
    rule.tool == "*" || rule.tool == tool_name(tool_call)
}

/// Check if a rule's path pattern matches the tool call's path.
fn matches_path(pattern: &Option<String>, tool_call: &ToolCall) -> bool {
    match pattern {
        None => true,
        Some(pat) => {
            let matcher = build_glob_matcher(pat);
            match (matcher, tool_path(tool_call)) {
                (Some(m), Some(path)) => m.is_match(path),
                (Some(_), None) => false,
                (None, _) => false,
            }
        }
    }
}

/// Build a glob matcher from a pattern string.
fn build_glob_matcher(pattern: &str) -> Option<GlobMatcher> {
    Glob::new(pattern).ok().map(|g| g.compile_matcher())
}

/// Check if a rule matches a tool call (both tool and path).
fn matches_rule(rule: &PermissionRule, tool_call: &ToolCall) -> bool {
    matches_tool(rule, tool_call) && matches_path(&rule.path_pattern, tool_call)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;

    // rtmx:req REQ-HITL-002
    #[test]
    fn default_trust_level_is_ask() {
        assert_eq!(TrustLevel::default(), TrustLevel::Ask);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn allow_rule_matches_exact_tool() {
        let engine = RulesEngine::new(
            TrustLevel::Ask,
            vec![PermissionRule {
                tool: "write_file".to_string(),
                path_pattern: None,
                effect: RuleEffect::Allow,
            }],
        );
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/lib.rs"),
            content: "code".into(),
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Allow);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn deny_rule_matches_exact_tool() {
        let engine = RulesEngine::new(
            TrustLevel::Ask,
            vec![PermissionRule {
                tool: "run_command".to_string(),
                path_pattern: None,
                effect: RuleEffect::Deny,
            }],
        );
        let call = ToolCall::RunCommand {
            command: "rm -rf /".into(),
            timeout_secs: 10,
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Deny);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn wildcard_tool_matches_any() {
        let engine = RulesEngine::new(
            TrustLevel::Ask,
            vec![PermissionRule {
                tool: "*".to_string(),
                path_pattern: None,
                effect: RuleEffect::Allow,
            }],
        );
        let call = ToolCall::RunCommand {
            command: "echo hi".into(),
            timeout_secs: 5,
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Allow);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn path_pattern_matches_glob() {
        let engine = RulesEngine::new(
            TrustLevel::Ask,
            vec![PermissionRule {
                tool: "write_file".to_string(),
                path_pattern: Some("src/**/*.rs".to_string()),
                effect: RuleEffect::Allow,
            }],
        );
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/lib.rs"),
            content: "code".into(),
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Allow);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn path_pattern_no_match() {
        let engine = RulesEngine::new(
            TrustLevel::Ask,
            vec![PermissionRule {
                tool: "write_file".to_string(),
                path_pattern: Some("src/**".to_string()),
                effect: RuleEffect::Allow,
            }],
        );
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("/tmp/foo"),
            content: "data".into(),
        };
        // Rule does not match, falls through to trust level (Ask -> AskUser)
        assert_eq!(engine.evaluate(&call), RuleDecision::AskUser);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn first_matching_rule_wins() {
        let engine = RulesEngine::new(
            TrustLevel::Ask,
            vec![
                PermissionRule {
                    tool: "write_file".to_string(),
                    path_pattern: None,
                    effect: RuleEffect::Deny,
                },
                PermissionRule {
                    tool: "write_file".to_string(),
                    path_pattern: None,
                    effect: RuleEffect::Allow,
                },
            ],
        );
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("any.txt"),
            content: "x".into(),
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Deny);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn no_matching_rule_returns_ask_user() {
        let engine = RulesEngine::new(TrustLevel::Ask, vec![]);
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("foo.txt"),
            content: "x".into(),
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::AskUser);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn trust_level_accept_edits_allows_writes() {
        let engine = RulesEngine::new(TrustLevel::AcceptEdits, vec![]);
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("foo.txt"),
            content: "x".into(),
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Allow);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn trust_level_accept_edits_asks_for_commands() {
        let engine = RulesEngine::new(TrustLevel::AcceptEdits, vec![]);
        let call = ToolCall::RunCommand {
            command: "make build".into(),
            timeout_secs: 60,
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::AskUser);
    }

    // rtmx:req REQ-HITL-002
    #[test]
    fn trust_level_full_auto_allows_everything() {
        let engine = RulesEngine::new(TrustLevel::FullAuto, vec![]);
        let call = ToolCall::RunCommand {
            command: "rm -rf /".into(),
            timeout_secs: 10,
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Allow);
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn grant_within_expiry_allows() {
        use crate::grants::create_grant;

        let grant = create_grant(
            PermissionRule {
                tool: "write_file".to_string(),
                path_pattern: None,
                effect: RuleEffect::Allow,
            },
            "session-1",
        );

        let engine = RulesEngine::new(TrustLevel::Ask, vec![]).with_grants(vec![grant]);

        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("anything.rs"),
            content: "code".into(),
        };
        assert_eq!(engine.evaluate(&call), RuleDecision::Allow);
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn expired_grant_ignored() {
        use chrono::{Duration, Utc};

        let mut grant = crate::grants::create_grant(
            PermissionRule {
                tool: "write_file".to_string(),
                path_pattern: None,
                effect: RuleEffect::Allow,
            },
            "session-1",
        );
        grant.expires_at = Utc::now() - Duration::hours(1);

        let engine = RulesEngine::new(TrustLevel::Ask, vec![]).with_grants(vec![grant]);

        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("anything.rs"),
            content: "code".into(),
        };
        // Expired grant is not matched, falls through to Ask -> AskUser
        assert_eq!(engine.evaluate(&call), RuleDecision::AskUser);
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn prune_expired_removes_old_grants() {
        use chrono::{Duration, Utc};

        let active = crate::grants::create_grant(
            PermissionRule {
                tool: "write_file".to_string(),
                path_pattern: None,
                effect: RuleEffect::Allow,
            },
            "active-session",
        );
        let mut expired = crate::grants::create_grant(
            PermissionRule {
                tool: "run_command".to_string(),
                path_pattern: None,
                effect: RuleEffect::Allow,
            },
            "expired-session",
        );
        expired.expires_at = Utc::now() - Duration::hours(1);

        let mut engine =
            RulesEngine::new(TrustLevel::Ask, vec![]).with_grants(vec![active, expired]);

        engine.prune_expired();

        // The run_command grant was expired and pruned, so it falls to Ask
        let cmd_call = ToolCall::RunCommand {
            command: "echo hello".into(),
            timeout_secs: 5,
        };
        assert_eq!(engine.evaluate(&cmd_call), RuleDecision::AskUser);

        // The write_file grant is still active
        let write_call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("foo.rs"),
            content: "x".into(),
        };
        assert_eq!(engine.evaluate(&write_call), RuleDecision::Allow);
    }
}
