//! Mock HITL approval gate for testing.

use aegis_domain::error::DomainError;
use aegis_domain::ports::ApprovalGate;
use aegis_domain::types::{ApprovalDecision, ToolCall};
use async_trait::async_trait;

/// A mock gate that auto-approves or auto-denies based on configuration.
pub struct MockApprovalGate {
    default_decision: ApprovalDecision,
}

impl MockApprovalGate {
    pub fn always_approve() -> Self {
        Self {
            default_decision: ApprovalDecision::Approved,
        }
    }

    pub fn always_deny() -> Self {
        Self {
            default_decision: ApprovalDecision::Denied,
        }
    }
}

#[async_trait]
impl ApprovalGate for MockApprovalGate {
    async fn request_approval(
        &self,
        _tool_call: &ToolCall,
    ) -> Result<ApprovalDecision, DomainError> {
        Ok(self.default_decision)
    }
}
