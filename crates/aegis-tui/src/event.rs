//! Unified TUI event type for the main event loop.
//!
//! All event sources (crossterm, agent stream, HITL approval, tick timer)
//! are funneled into a single `TuiEvent` enum so the event loop can use
//! a single channel receiver with `tokio::select!`-free dispatch.

use aegis_domain::types::ToolCall;

/// An event consumed by the TUI event loop.
#[derive(Debug)]
pub enum TuiEvent {
    /// Crossterm terminal event (keyboard, resize, mouse).
    Terminal(crossterm::event::Event),
    /// A token arrived from the agent's LLM stream.
    AgentToken(String),
    /// Agent proposed a tool call (displayed inline in chat log).
    AgentToolUse(ToolCall),
    /// Agent completed with final token counts.
    AgentDone {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// Agent stream encountered an error.
    AgentError(String),
    /// HITL approval request from the agent (blocks agent until resolved).
    ApprovalRequest(ApprovalRequestHandle),
    /// Animation tick (~150ms interval).
    Tick,
}

/// A pending HITL approval request with the channel to send the decision back.
pub struct ApprovalRequestHandle {
    pub tool_call: ToolCall,
    pub description: String,
    pub response_tx: tokio::sync::oneshot::Sender<aegis_domain::types::ApprovalDecision>,
}

// Manual Debug impl because oneshot::Sender doesn't implement Debug.
impl std::fmt::Debug for ApprovalRequestHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalRequestHandle")
            .field("tool_call", &self.tool_call)
            .field("description", &self.description)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;

    // @req REQ-TUI-001
    #[test]
    fn tui_event_variants_are_constructible() {
        let token = TuiEvent::AgentToken("hello".to_string());
        assert!(matches!(token, TuiEvent::AgentToken(_)));

        let done = TuiEvent::AgentDone {
            input_tokens: 100,
            output_tokens: 200,
        };
        assert!(matches!(done, TuiEvent::AgentDone { .. }));

        let error = TuiEvent::AgentError("timeout".to_string());
        assert!(matches!(error, TuiEvent::AgentError(_)));

        let tick = TuiEvent::Tick;
        assert!(matches!(tick, TuiEvent::Tick));

        let tool = TuiEvent::AgentToolUse(ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        });
        assert!(matches!(tool, TuiEvent::AgentToolUse(_)));
    }

    // @req REQ-TUI-001
    #[test]
    fn approval_request_handle_is_debuggable() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let handle = ApprovalRequestHandle {
            tool_call: ToolCall::RunCommand {
                command: "cargo test".to_string(),
                timeout_secs: 60,
            },
            description: "Execute: cargo test (timeout: 60s)".to_string(),
            response_tx: tx,
        };
        let debug = format!("{handle:?}");
        assert!(debug.contains("ApprovalRequestHandle"));
        assert!(debug.contains("cargo test"));
    }
}
