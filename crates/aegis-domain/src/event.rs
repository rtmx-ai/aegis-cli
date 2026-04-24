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
    /// Emergency kill switch activated by the operator (Ctrl+K).
    /// Halts the agent loop and denies all queued tool calls.
    KillSwitch {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
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
