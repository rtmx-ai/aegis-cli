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

        // File-picker intercept: capture keystrokes when picker is open.
        if self.file_picker.is_some() {
            return self.handle_file_picker_key(key);
        }

        // Search-mode intercept: capture keystrokes when search is active.
        if self.input.in_search_mode() {
            match key.code {
                KeyCode::Char(c) => {
                    self.input.search_insert_char(c);
                    self.recompute_search_match();
                    return Action::Continue;
                }
                KeyCode::Backspace => {
                    self.input.search_backspace();
                    self.recompute_search_match();
                    return Action::Continue;
                }
                KeyCode::Esc => {
                    self.input.exit_search_mode();
                    self.search_match_index = None;
                    return Action::Continue;
                }
                _ => return Action::Continue,
            }
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
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.enter_search_mode();
                Action::Continue
            }
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
            // Vim normal mode: n/N cycle search matches forward/backward
            KeyCode::Char('n') if self.input.mode == crate::input::InputMode::Normal => {
                self.cycle_search_match(true);
                Action::Continue
            }
            KeyCode::Char('N') if self.input.mode == crate::input::InputMode::Normal => {
                self.cycle_search_match(false);
                Action::Continue
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(super::PAGE_SCROLL_LINES);
                self.auto_scroll = false;
                Action::Continue
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(super::PAGE_SCROLL_LINES);
                if self.scroll_offset == 0 {
                    self.auto_scroll = true;
                }
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
            KeyCode::Char('@') => {
                self.input.insert_char('@');
                self.open_file_picker();
                Action::Continue
            }
            KeyCode::Char(c) => {
                self.input.insert_char(c);
                Action::Continue
            }
            _ => Action::Continue,
        }
    }

    /// Open the file picker by scanning the current directory.
    fn open_file_picker(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_default();
        self.file_picker = Some(super::file_picker::FilePicker::open("", &cwd));
    }

    /// Handle a key event while the file picker is open.
    fn handle_file_picker_key(&mut self, key: KeyEvent) -> Action {
        let cwd = std::env::current_dir().unwrap_or_default();
        let picker = match self.file_picker.as_mut() {
            Some(p) => p,
            None => return Action::Continue,
        };

        match key.code {
            KeyCode::Esc => {
                // Close picker and remove the @ from input
                self.file_picker = None;
                self.input.backspace();
                Action::Continue
            }
            KeyCode::Enter => {
                // If selected entry is a directory, toggle expand.
                if picker.toggle_expand(&cwd) {
                    return Action::Continue;
                }
                // Otherwise insert selected file path, replacing the @.
                let path = picker.selected_path();
                self.file_picker = None;
                if let Some(path) = path {
                    // Remove the @ character we inserted
                    self.input.backspace();
                    // Insert the file path
                    self.input.insert_str(&path);
                }
                Action::Continue
            }
            KeyCode::Up => {
                picker.select_prev();
                Action::Continue
            }
            KeyCode::Down => {
                picker.select_next();
                Action::Continue
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                picker.select_prev();
                Action::Continue
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                picker.select_next();
                Action::Continue
            }
            KeyCode::Backspace => {
                let mut query = picker.query.clone();
                if query.is_empty() {
                    // Close picker and remove @
                    self.file_picker = None;
                    self.input.backspace();
                } else {
                    query.pop();
                    picker.update_query(&query, &cwd);
                }
                Action::Continue
            }
            KeyCode::Char(c) => {
                let mut query = picker.query.clone();
                query.push(c);

                // Detect @git: prefix -- switch to git-aware mode.
                if picker.mode == super::file_picker::PickerMode::FileSystem && query == "git:" {
                    let git_picker = super::file_picker::FilePicker::open_git(&cwd);
                    *picker = git_picker;
                    return Action::Continue;
                }

                picker.update_query(&query, &cwd);
                Action::Continue
            }
            _ => Action::Continue,
        }
    }

    /// Find the first message whose content matches the search query
    /// (case-insensitive). Sets `search_match_index` accordingly.
    fn recompute_search_match(&mut self) {
        let query = match self.input.search_query() {
            Some(q) if !q.is_empty() => q.to_lowercase(),
            _ => {
                self.search_match_index = None;
                return;
            }
        };
        self.search_match_index = self
            .messages
            .iter()
            .position(|m| m.content.to_lowercase().contains(&query));
    }

    /// Cycle the search match forward or backward, wrapping around.
    fn cycle_search_match(&mut self, forward: bool) {
        let query = match self.input.search_query() {
            Some(q) if !q.is_empty() => q.to_lowercase(),
            _ => return,
        };
        let len = self.messages.len();
        if len == 0 {
            return;
        }
        let start = match self.search_match_index {
            Some(idx) => {
                if forward {
                    (idx + 1) % len
                } else {
                    (idx + len - 1) % len
                }
            }
            None => 0,
        };
        for i in 0..len {
            let idx = if forward {
                (start + i) % len
            } else {
                (start + len - i) % len
            };
            if self.messages[idx].content.to_lowercase().contains(&query) {
                self.search_match_index = Some(idx);
                return;
            }
        }
        self.search_match_index = None;
    }
}
