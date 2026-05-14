//! HITL approval display and key handling.

use super::{Action, App, AppPhase};
use aegis_domain::types::{ApprovalDecision, ApprovalResponse, ToolRisk};
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
        match key.code {
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // REQ-HITL-017: Enter edit mode with args pre-populated.
                if let Some(ref info) = self.approval_display {
                    let args_text = info.args_summary.clone();
                    self.editing_approval_args = Some(args_text.clone());
                    // Load args into the input textarea for editing
                    self.input.text = args_text;
                    self.input.cursor = self.input.text.len();
                    self.phase = AppPhase::EditingApproval;
                }
                return Action::Continue;
            }
            _ => {}
        }

        let decision = match key.code {
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('y') | KeyCode::Char('Y') => {
                Some(ApprovalDecision::Approved)
            }
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Char('n') | KeyCode::Char('N') => {
                Some(ApprovalDecision::Denied)
            }
            KeyCode::Char('s') | KeyCode::Char('S') => Some(ApprovalDecision::Skipped),
            _ => None,
        };

        if let Some(decision) = decision {
            self.send_approval_response(ApprovalResponse::simple(decision));
        }

        Action::Continue
    }

    /// Handle key input while editing approval args (REQ-HITL-017).
    pub(crate) fn handle_editing_approval_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Enter => {
                // Confirm edit: send Edited with modified args
                let edited_text = self.input.text.clone();
                self.input.text.clear();
                self.input.cursor = 0;
                self.editing_approval_args = None;
                self.send_approval_response(ApprovalResponse::edited(edited_text));
            }
            KeyCode::Esc => {
                // Cancel edit: return to approval modal
                self.input.text.clear();
                self.input.cursor = 0;
                self.editing_approval_args = None;
                self.phase = AppPhase::AwaitingApproval;
            }
            KeyCode::Char(c) => {
                self.input.insert_char(c);
            }
            KeyCode::Backspace => {
                self.input.backspace();
            }
            KeyCode::Left => {
                self.input.move_left();
            }
            KeyCode::Right => {
                self.input.move_right();
            }
            KeyCode::Home => {
                self.input.move_home();
            }
            KeyCode::End => {
                self.input.move_end();
            }
            _ => {}
        }
        Action::Continue
    }

    /// Send an approval response and transition phase.
    fn send_approval_response(&mut self, response: ApprovalResponse) {
        let decision = response.decision;
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
            let _ = handle.response_tx.send(response);
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
}
