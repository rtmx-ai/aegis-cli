//! Keyboard input handling for idle mode (and phase dispatch).

use super::{Action, App, AppPhase};
use crate::messages::ChatMessage;
use crate::slash_commands;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

impl App {
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        agent_tx: &mpsc::UnboundedSender<String>,
    ) -> Action {
        // Debug: log every key event when /keylog is active
        if self.keylog {
            self.messages.push(ChatMessage::system(format!(
                "[keylog] code={:?} modifiers={:?} kind={:?}",
                key.code, key.modifiers, key.kind,
            )));
        }

        // Phase-specific key handling
        match self.phase {
            AppPhase::Splash => {
                // Any keypress dismisses the splash screen
                self.phase = AppPhase::Idle;
                return Action::Continue;
            }
            AppPhase::AwaitingApproval => return self.handle_approval_key(key),
            AppPhase::Streaming | AppPhase::ToolExecuting => {
                // Ctrl+C cancels (handled by caller via cancel_token)
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return Action::Quit;
                }
                return Action::Continue;
            }
            AppPhase::Idle => {}
        }

        // Idle-phase key handling
        match key.code {
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                let text = self.input.submit();
                if text.is_empty() {
                    return Action::Continue;
                }
                // Check for slash commands
                match slash_commands::parse_slash_command(&text) {
                    slash_commands::ParseResult::Command(cmd) => self.execute_slash_command(cmd),
                    slash_commands::ParseResult::NotACommand => {
                        self.messages.push(ChatMessage::user(&text));
                        self.phase = AppPhase::Streaming;
                        self.stream_buffer.clear();
                        let _ = agent_tx.send(text);
                        Action::Continue
                    }
                    slash_commands::ParseResult::Unknown(name) => {
                        self.messages.push(ChatMessage::error(format!(
                            "Unknown command: {name}. Type /help for commands."
                        )));
                        Action::Continue
                    }
                }
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.insert_newline();
                Action::Continue
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                self.input.insert_newline();
                Action::Continue
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                match self.input.paste_from_clipboard() {
                    Ok(true) => {}
                    Ok(false) => {} // Empty clipboard, silently ignore
                    Err(e) => {
                        tracing::warn!(error = %e, "clipboard paste failed");
                        self.messages.push(ChatMessage::error(e));
                    }
                }
                Action::Continue
            }
            KeyCode::Esc => {
                if self.input.mode == crate::input::InputMode::Insert {
                    self.input.enter_normal_mode();
                } else {
                    self.input.enter_insert_mode();
                }
                Action::Continue
            }
            // Vim normal mode: 'o' opens new line below cursor and enters insert
            KeyCode::Char('o') if self.input.mode == crate::input::InputMode::Normal => {
                self.input.enter_insert_mode();
                self.input.move_end();
                self.input.insert_newline();
                Action::Continue
            }
            // Vim normal mode: 'i' enters insert mode (explicit for clarity)
            KeyCode::Char('i') if self.input.mode == crate::input::InputMode::Normal => {
                self.input.enter_insert_mode();
                Action::Continue
            }
            KeyCode::Up => {
                self.input.history_prev();
                Action::Continue
            }
            KeyCode::Down => {
                self.input.history_next();
                Action::Continue
            }
            KeyCode::Backspace => {
                self.input.backspace();
                Action::Continue
            }
            KeyCode::Left => {
                self.input.move_left();
                Action::Continue
            }
            KeyCode::Right => {
                self.input.move_right();
                Action::Continue
            }
            KeyCode::Home => {
                self.input.move_home();
                Action::Continue
            }
            KeyCode::End => {
                self.input.move_end();
                Action::Continue
            }
            KeyCode::Char(c) => {
                self.input.insert_char(c);
                Action::Continue
            }
            _ => Action::Continue,
        }
    }
}
