//! Mock HITL approval gate for testing.

use aegis_domain::error::DomainError;
use aegis_domain::ports::ApprovalGate;
use aegis_domain::types::{ApprovalDecision, ApprovalResponse, ToolCall};
use async_trait::async_trait;

/// A mock gate that auto-approves or auto-denies based on configuration.
pub struct MockApprovalGate {
    default_decision: ApprovalDecision,
    edited_args: Option<String>,
}

impl MockApprovalGate {
    pub fn always_approve() -> Self {
        Self {
            default_decision: ApprovalDecision::Approved,
            edited_args: None,
        }
    }

    pub fn always_deny() -> Self {
        Self {
            default_decision: ApprovalDecision::Denied,
            edited_args: None,
        }
    }

    pub fn always_skip() -> Self {
        Self {
            default_decision: ApprovalDecision::Skipped,
            edited_args: None,
        }
    }

    pub fn with_decision(decision: ApprovalDecision) -> Self {
        Self {
            default_decision: decision,
            edited_args: None,
        }
    }

    /// Create a gate that approves with edited args (REQ-HITL-017).
    pub fn approve_with_edit(edited_args: &str) -> Self {
        Self {
            default_decision: ApprovalDecision::Edited,
            edited_args: Some(edited_args.to_string()),
        }
    }
}

#[async_trait]
impl ApprovalGate for MockApprovalGate {
    async fn request_approval(
        &self,
        _tool_call: &ToolCall,
    ) -> Result<ApprovalResponse, DomainError> {
        Ok(match &self.edited_args {
            Some(args) => ApprovalResponse::edited(args.clone()),
            None => ApprovalResponse::simple(self.default_decision),
        })
    }
}
