//! Domain events emitted by bounded contexts.

use crate::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Domain events emitted during agent operation.
///
/// # Examples
///
/// Construct a `SessionStarted` event:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::event::DomainEvent;
/// use aegis_domain::types::SessionId;
/// use chrono::Utc;
///
/// let event = DomainEvent::SessionStarted {
///     session_id: SessionId::new(),
///     timestamp: Utc::now(),
/// };
/// ```
///
/// Construct a `SessionEnded` event:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::event::DomainEvent;
/// use aegis_domain::types::SessionId;
/// use chrono::Utc;
///
/// let event = DomainEvent::SessionEnded {
///     session_id: SessionId::new(),
///     timestamp: Utc::now(),
/// };
/// ```
///
/// Construct a `RequirementLinked` event:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::event::DomainEvent;
/// use aegis_domain::types::{SessionId, RequirementId};
/// use chrono::Utc;
///
/// let event = DomainEvent::RequirementLinked {
///     session_id: SessionId::new(),
///     requirement_id: RequirementId::new("REQ-BUILD-001"),
///     timestamp: Utc::now(),
/// };
/// ```
///
/// Construct a `KillSwitch` event:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::event::DomainEvent;
/// use aegis_domain::types::SessionId;
/// use chrono::Utc;
///
/// let event = DomainEvent::KillSwitch {
///     session_id: SessionId::new(),
///     timestamp: Utc::now(),
///     pending_tool_count: 3,
/// };
/// ```
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
    /// Emergency kill switch activated by the operator (Ctrl+K).
    /// Halts the agent loop and denies all queued tool calls.
    KillSwitch {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
        /// Number of pending tool calls denied when the kill switch fired.
        pending_tool_count: usize,
    },
    /// A CUI-marked message was blocked from transmission to a commercial endpoint.
    CuiBlocked {
        session_id: SessionId,
        endpoint_url: String,
        pattern_matched: String,
        timestamp: DateTime<Utc>,
    },
    /// Model origin policy decision logged on model switch, session
    /// start, or download attempt (REQ-SECURITY-027).
    ModelPolicyDecision {
        session_id: String,
        model_name: String,
        origin_country: String,
        origin_tier: String,
        decision: String,
        reason: String,
        timestamp: String,
    },
    /// Model selection audit event with BOM snapshot (REQ-LLM-053).
    ModelSelected {
        session_id: String,
        model_name: String,
        /// JSON-serialized AiBom snapshot.
        bom_snapshot: String,
        /// BOM policy decision tier (Approved/ReviewRequired/Denied).
        policy_decision: String,
        /// Accumulated policy reasons.
        policy_reasons: Vec<String>,
        timestamp: String,
    },
    /// Token usage for a single LLM turn, with provider attribution.
    TokensConsumed {
        session_id: String,
        provider_kind: String,
        model: String,
        project_id: Option<String>,
        region: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        timestamp: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tokens_consumed_event() -> DomainEvent {
        DomainEvent::TokensConsumed {
            session_id: "sess-001".to_string(),
            provider_kind: "vertex".to_string(),
            model: "gemini-2.5-pro".to_string(),
            project_id: Some("my-project-123".to_string()),
            region: Some("us-central1".to_string()),
            input_tokens: 1000,
            output_tokens: 500,
            timestamp: "2026-04-18T12:00:00Z".to_string(),
        }
    }

    // rtmx:req REQ-AUDIT-024
    #[test]
    fn test_tokens_consumed_event_carries_provider_context() {
        let event = make_tokens_consumed_event();
        if let DomainEvent::TokensConsumed {
            session_id,
            provider_kind,
            model,
            project_id,
            region,
            input_tokens,
            output_tokens,
            timestamp,
        } = &event
        {
            assert_eq!(session_id, "sess-001");
            assert_eq!(provider_kind, "vertex");
            assert_eq!(model, "gemini-2.5-pro");
            assert_eq!(project_id.as_deref(), Some("my-project-123"));
            assert_eq!(region.as_deref(), Some("us-central1"));
            assert_eq!(*input_tokens, 1000);
            assert_eq!(*output_tokens, 500);
            assert_eq!(timestamp, "2026-04-18T12:00:00Z");
        } else {
            panic!("Expected TokensConsumed variant");
        }
    }

    // rtmx:req REQ-AUDIT-024
    #[test]
    fn test_tokens_consumed_serializes_to_json() {
        let event = make_tokens_consumed_event();
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"provider_kind\":\"vertex\""));
        assert!(json.contains("\"model\":\"gemini-2.5-pro\""));
        assert!(json.contains("\"project_id\":\"my-project-123\""));
        assert!(json.contains("\"input_tokens\":1000"));
        assert!(json.contains("\"output_tokens\":500"));
    }

    // rtmx:req REQ-AUDIT-024
    #[test]
    fn test_tokens_consumed_with_none_project() {
        let event = DomainEvent::TokensConsumed {
            session_id: "sess-002".to_string(),
            provider_kind: "local".to_string(),
            model: "llama3".to_string(),
            project_id: None,
            region: None,
            input_tokens: 200,
            output_tokens: 100,
            timestamp: "2026-04-18T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"project_id\":null"));
        assert!(json.contains("\"region\":null"));
        // Verify it round-trips
        let deserialized: DomainEvent = serde_json::from_str(&json).unwrap();
        if let DomainEvent::TokensConsumed { project_id, .. } = deserialized {
            assert!(project_id.is_none());
        } else {
            panic!("Expected TokensConsumed variant");
        }
    }
}
