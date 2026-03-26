//! Multi-line input with vim mode and command history.
//!
//! Provides an InputState that handles text editing with two modes:
//! Insert (typing) and Normal (vim navigation). Supports command
//! history via Up/Down arrows.

/// Input editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Typing text (default).
    Insert,
    /// Vim-style navigation (Esc to enter, i to exit).
    Normal,
}

/// State for the multi-line input field.
#[derive(Debug, Clone)]
pub struct InputState {
    /// Current text content.
    pub text: String,
    /// Cursor position (byte offset into text).
    pub cursor: usize,
    /// Current editing mode.
    pub mode: InputMode,
    /// Command history (newest last).
    history: Vec<String>,
    /// Current history navigation index (None = editing new input).
    history_index: Option<usize>,
    /// Saved text when navigating history.
    saved_text: String,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            mode: InputMode::Insert,
            history: Vec::new(),
            history_index: None,
            saved_text: String::new(),
        }
    }
}

impl InputState {
    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        if self.mode != InputMode::Insert {
            return;
        }
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Find the previous char boundary
        let prev = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.text.drain(prev..self.cursor);
        self.cursor = prev;
    }

    /// Move cursor left by one character.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right by one character.
    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += self.text[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
        }
    }

    /// Move to start of line.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move to end of line.
    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Switch to Normal mode (Esc).
    pub fn enter_normal_mode(&mut self) {
        self.mode = InputMode::Normal;
    }

    /// Switch to Insert mode (i).
    pub fn enter_insert_mode(&mut self) {
        self.mode = InputMode::Insert;
    }

    /// Insert a newline (Shift+Enter).
    pub fn insert_newline(&mut self) {
        if self.mode == InputMode::Insert {
            self.insert_char('\n');
        }
    }

    /// Submit the current text: add to history and return it.
    /// Clears the input state.
    pub fn submit(&mut self) -> String {
        let text = self.text.clone();
        if !text.trim().is_empty() {
            self.history.push(text.clone());
        }
        self.text.clear();
        self.cursor = 0;
        self.history_index = None;
        self.saved_text.clear();
        self.mode = InputMode::Insert;
        text
    }

    /// Navigate to the previous command in history (Up arrow).
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                // Save current text and go to last history entry
                self.saved_text = self.text.clone();
                let idx = self.history.len() - 1;
                self.history_index = Some(idx);
                self.text = self.history[idx].clone();
                self.cursor = self.text.len();
            }
            Some(idx) if idx > 0 => {
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.text = self.history[new_idx].clone();
                self.cursor = self.text.len();
            }
            _ => {} // Already at oldest
        }
    }

    /// Navigate to the next command in history (Down arrow).
    pub fn history_next(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                let new_idx = idx + 1;
                self.history_index = Some(new_idx);
                self.text = self.history[new_idx].clone();
                self.cursor = self.text.len();
            } else {
                // Back to saved text
                self.history_index = None;
                self.text = self.saved_text.clone();
                self.cursor = self.text.len();
            }
        }
    }

    /// Get the history entries.
    pub fn history(&self) -> &[String] {
        &self.history
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-TUI-004
    #[test]
    fn insert_and_read_text() {
        let mut input = InputState::default();
        input.insert_char('h');
        input.insert_char('i');
        assert_eq!(input.text, "hi");
        assert_eq!(input.cursor, 2);
    }

    // @req REQ-TUI-004
    #[test]
    fn backspace_removes_character() {
        let mut input = InputState::default();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');
        input.backspace();
        assert_eq!(input.text, "ab");
        assert_eq!(input.cursor, 2);
    }

    // @req REQ-TUI-004
    #[test]
    fn backspace_at_start_does_nothing() {
        let mut input = InputState::default();
        input.backspace();
        assert_eq!(input.text, "");
        assert_eq!(input.cursor, 0);
    }

    // @req REQ-TUI-004
    #[test]
    fn cursor_movement() {
        let mut input = InputState::default();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');
        assert_eq!(input.cursor, 3);

        input.move_left();
        assert_eq!(input.cursor, 2);

        input.move_left();
        assert_eq!(input.cursor, 1);

        input.move_right();
        assert_eq!(input.cursor, 2);

        input.move_home();
        assert_eq!(input.cursor, 0);

        input.move_end();
        assert_eq!(input.cursor, 3);
    }

    // @req REQ-TUI-004
    #[test]
    fn insert_at_cursor_position() {
        let mut input = InputState::default();
        input.insert_char('a');
        input.insert_char('c');
        input.move_left(); // cursor at 'c'
        input.insert_char('b');
        assert_eq!(input.text, "abc");
    }

    // @req REQ-TUI-004
    #[test]
    fn vim_mode_toggle() {
        let mut input = InputState::default();
        assert_eq!(input.mode, InputMode::Insert);

        input.enter_normal_mode();
        assert_eq!(input.mode, InputMode::Normal);

        // Typing should be ignored in Normal mode
        input.insert_char('x');
        assert_eq!(input.text, "");

        input.enter_insert_mode();
        assert_eq!(input.mode, InputMode::Insert);
        input.insert_char('x');
        assert_eq!(input.text, "x");
    }

    // @req REQ-TUI-004
    #[test]
    fn multi_line_input() {
        let mut input = InputState::default();
        input.insert_char('a');
        input.insert_newline();
        input.insert_char('b');
        assert_eq!(input.text, "a\nb");
    }

    // @req REQ-TUI-004
    #[test]
    fn submit_returns_text_and_clears() {
        let mut input = InputState::default();
        input.insert_char('h');
        input.insert_char('i');

        let submitted = input.submit();
        assert_eq!(submitted, "hi");
        assert_eq!(input.text, "");
        assert_eq!(input.cursor, 0);
    }

    // @req REQ-TUI-004
    #[test]
    fn submit_adds_to_history() {
        let mut input = InputState::default();
        input.insert_char('a');
        input.submit();
        input.insert_char('b');
        input.submit();

        assert_eq!(input.history(), &["a", "b"]);
    }

    // @req REQ-TUI-004
    #[test]
    fn empty_submit_not_added_to_history() {
        let mut input = InputState::default();
        input.submit();
        assert!(input.history().is_empty());
    }

    // @req REQ-TUI-004
    #[test]
    fn history_navigation_up_down() {
        let mut input = InputState::default();
        input.insert_char('a');
        input.submit();
        input.insert_char('b');
        input.submit();
        input.insert_char('c');
        input.submit();

        // Type something new
        input.insert_char('d');

        // Up: go to 'c'
        input.history_prev();
        assert_eq!(input.text, "c");

        // Up: go to 'b'
        input.history_prev();
        assert_eq!(input.text, "b");

        // Down: back to 'c'
        input.history_next();
        assert_eq!(input.text, "c");

        // Down: back to saved 'd'
        input.history_next();
        assert_eq!(input.text, "d");

        // Down again: no change
        input.history_next();
        assert_eq!(input.text, "d");
    }

    // @req REQ-TUI-004
    #[test]
    fn history_up_on_empty_does_nothing() {
        let mut input = InputState::default();
        input.history_prev();
        assert_eq!(input.text, "");
    }

    // @req REQ-TUI-004
    #[test]
    fn submit_resets_mode_to_insert() {
        let mut input = InputState::default();
        input.enter_normal_mode();
        input.text = "test".to_string();
        input.submit();
        assert_eq!(input.mode, InputMode::Insert);
    }

    // @req REQ-TUI-004
    #[test]
    fn handles_utf8_characters() {
        let mut input = InputState::default();
        input.insert_char('h');
        input.insert_char('e');
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');
        assert_eq!(input.cursor, 5);

        input.move_left();
        input.move_left();
        input.backspace();
        assert_eq!(input.text, "helo");
    }
}
