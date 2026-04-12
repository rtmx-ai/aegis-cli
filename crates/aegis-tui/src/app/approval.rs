//! HITL approval display and key handling.

use super::{Action, App, AppPhase};
use aegis_domain::types::{ApprovalDecision, ToolRisk};
use crossterm::event::{KeyCode, KeyEvent};

/// Information about a pending HITL approval displayed as a modal overlay.
#[derive(Debug, Clone)]
pub struct ApprovalDisplayInfo {
    /// Name of the tool requesting approval.
    pub tool_name: String,
    /// Summary of the tool's arguments.
    pub args_summary: String,
    /// Risk level of the tool call.
    pub risk: ToolRisk,
}

impl App {
    pub(crate) fn handle_approval_key(&mut self, key: KeyEvent) -> Action {
        let decision = match key.code {
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('y') | KeyCode::Char('Y') => {
                Some(ApprovalDecision::Approved)
            }
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Char('n') | KeyCode::Char('N') => {
                Some(ApprovalDecision::Denied)
            }
            KeyCode::Char('s') | KeyCode::Char('S') => Some(ApprovalDecision::Skipped),
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Edit not yet implemented; treat as approve
                Some(ApprovalDecision::Approved)
            }
            _ => None,
        };

        if let Some(decision) = decision {
            self.approval_display = None;
            if let Some(handle) = self.pending_approval.take() {
                let decision_label = match decision {
                    ApprovalDecision::Approved => "Approved",
                    ApprovalDecision::Denied => "Denied",
                    ApprovalDecision::Skipped => "Skipped",
                    ApprovalDecision::Edited => "Approved (edited)",
                    ApprovalDecision::TimedOut => "Timed out (denied)",
                };
                self.messages
                    .push(crate::messages::ChatMessage::system(format!(
                        "[{decision_label}]",
                    )));
                let _ = handle.response_tx.send(decision);
            }
            self.phase = if matches!(
                decision,
                ApprovalDecision::Approved | ApprovalDecision::Edited
            ) {
                AppPhase::ToolExecuting
            } else {
                AppPhase::Streaming
            };
        }

        Action::Continue
    }
}
