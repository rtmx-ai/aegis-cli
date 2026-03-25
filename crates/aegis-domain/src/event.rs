//! Domain events emitted by bounded contexts.

use crate::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    SessionStarted {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },
    SessionEnded {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },
    ToolCallProposed {
        session_id: SessionId,
        request_id: RequestId,
        tool_call: ToolCall,
        timestamp: DateTime<Utc>,
    },
    ToolCallApproved {
        session_id: SessionId,
        request_id: RequestId,
        decision: ApprovalDecision,
        timestamp: DateTime<Utc>,
    },
    ToolCallExecuted {
        session_id: SessionId,
        request_id: RequestId,
        result: ToolResult,
        timestamp: DateTime<Utc>,
    },
    RequirementLinked {
        session_id: SessionId,
        requirement_id: RequirementId,
        timestamp: DateTime<Utc>,
    },
}
