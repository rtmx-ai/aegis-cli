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
    /// Active search query (Some when in search mode, None otherwise).
    search_query: Option<String>,
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
            search_query: None,
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

    /// Maximum paste size in bytes (64 KB).
    const MAX_PASTE_BYTES: usize = 64 * 1024;

    /// Insert a string at the cursor position, truncating at 64 KB.
    pub fn insert_str(&mut self, s: &str) {
        if self.mode != InputMode::Insert {
            return;
        }
        let truncated = if s.len() > Self::MAX_PASTE_BYTES {
            // Truncate at a char boundary
            let mut end = Self::MAX_PASTE_BYTES;
            while end > 0 && !s.is_char_boundary(end) {
                end -= 1;
            }
            &s[..end]
        } else {
            s
        };
        self.text.insert_str(self.cursor, truncated);
        self.cursor += truncated.len();
    }

    /// Sanitize pasted text: strip non-printable control characters except
    /// newline and tab, then truncate at 64 KB.
    pub fn sanitize_paste(text: &str) -> String {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect();
        if cleaned.len() <= Self::MAX_PASTE_BYTES {
            cleaned
        } else {
            let mut end = Self::MAX_PASTE_BYTES;
            while end > 0 && !cleaned.is_char_boundary(end) {
                end -= 1;
            }
            cleaned[..end].to_string()
        }
    }

    /// Insert sanitized pasted text at the cursor position.
    pub fn insert_paste(&mut self, raw: &str) {
        let cleaned = Self::sanitize_paste(raw);
        if !cleaned.is_empty() {
            self.insert_str(&cleaned);
        }
    }

    /// Paste from the system clipboard. Returns Ok(true) if pasted,
    /// Ok(false) if clipboard was empty, Err on clipboard access failure.
    pub fn paste_from_clipboard(&mut self) -> Result<bool, String> {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("clipboard error: {e}"))?;
        match clipboard.get_text() {
            Ok(text) if text.is_empty() => Ok(false),
            Ok(text) => {
                self.insert_str(&text);
                Ok(true)
            }
            Err(arboard::Error::ContentNotAvailable) => Ok(false),
            Err(e) => Err(format!("clipboard read error: {e}")),
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

    /// Enter search mode (Ctrl+F).
    pub fn enter_search_mode(&mut self) {
        self.search_query = Some(String::new());
    }

    /// Exit search mode (Esc while searching).
    pub fn exit_search_mode(&mut self) {
        self.search_query = None;
    }

    /// Get the current search query, if in search mode.
    pub fn search_query(&self) -> Option<&str> {
        self.search_query.as_deref()
    }

    /// Whether the input is currently in search mode.
    pub fn in_search_mode(&self) -> bool {
        self.search_query.is_some()
    }

    /// Append a character to the search query.
    pub fn search_insert_char(&mut self, ch: char) {
        if let Some(ref mut q) = self.search_query {
            q.push(ch);
        }
    }

    /// Remove the last character from the search query.
    pub fn search_backspace(&mut self) {
        if let Some(ref mut q) = self.search_query {
            q.pop();
        }
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

    // @req REQ-TUI-034
    #[test]
    fn sanitize_paste_strips_control_chars() {
        let raw = "hello\x00world\x07test\nkeep\ttabs";
        let cleaned = InputState::sanitize_paste(raw);
        assert_eq!(cleaned, "helloworldtest\nkeep\ttabs");
    }

    // @req REQ-TUI-034
    #[test]
    fn sanitize_paste_preserves_newlines_and_tabs() {
        let raw = "line1\nline2\tindented";
        let cleaned = InputState::sanitize_paste(raw);
        assert_eq!(cleaned, raw);
    }

    // @req REQ-TUI-034
    #[test]
    fn sanitize_paste_truncates_at_64kb() {
        let big = "x".repeat(128 * 1024);
        let cleaned = InputState::sanitize_paste(&big);
        assert_eq!(cleaned.len(), 64 * 1024);
    }

    // @req REQ-TUI-034
    #[test]
    fn insert_paste_sanitizes_and_inserts() {
        let mut input = InputState::default();
        input.insert_paste("hello\x00world");
        assert_eq!(input.text, "helloworld");
    }

    // @req REQ-TUI-022
    #[test]
    fn insert_str_at_cursor() {
        let mut input = InputState::default();
        input.insert_char('a');
        input.insert_char('d');
        input.move_left(); // cursor before 'd'
        input.insert_str("bc");
        assert_eq!(input.text, "abcd");
        assert_eq!(input.cursor, 3); // after "abc"
    }

    // @req REQ-TUI-022
    #[test]
    fn insert_str_multiline() {
        let mut input = InputState::default();
        input.insert_str("line1\nline2\nline3");
        assert_eq!(input.text, "line1\nline2\nline3");
    }

    // @req REQ-TUI-022
    #[test]
    fn insert_str_truncates_at_64kb() {
        let mut input = InputState::default();
        let big = "x".repeat(128 * 1024);
        input.insert_str(&big);
        assert_eq!(input.text.len(), 64 * 1024);
    }

    // @req REQ-TUI-022
    #[test]
    fn insert_str_ignored_in_normal_mode() {
        let mut input = InputState::default();
        input.enter_normal_mode();
        input.insert_str("hello");
        assert_eq!(input.text, "");
    }

    // @req REQ-TUI-017
    #[test]
    fn enter_search_mode_sets_empty_query() {
        let mut input = InputState::default();
        assert!(!input.in_search_mode());
        input.enter_search_mode();
        assert!(input.in_search_mode());
        assert_eq!(input.search_query(), Some(""));
    }

    // @req REQ-TUI-017
    #[test]
    fn exit_search_mode_clears_query() {
        let mut input = InputState::default();
        input.enter_search_mode();
        input.search_insert_char('a');
        input.exit_search_mode();
        assert!(!input.in_search_mode());
        assert_eq!(input.search_query(), None);
    }

    // @req REQ-TUI-017
    #[test]
    fn search_insert_char_appends_to_query() {
        let mut input = InputState::default();
        input.enter_search_mode();
        input.search_insert_char('h');
        input.search_insert_char('i');
        assert_eq!(input.search_query(), Some("hi"));
    }

    // @req REQ-TUI-017
    #[test]
    fn search_backspace_removes_last_char() {
        let mut input = InputState::default();
        input.enter_search_mode();
        input.search_insert_char('a');
        input.search_insert_char('b');
        input.search_backspace();
        assert_eq!(input.search_query(), Some("a"));
    }

    // @req REQ-TUI-017
    #[test]
    fn search_backspace_on_empty_query_does_nothing() {
        let mut input = InputState::default();
        input.enter_search_mode();
        input.search_backspace();
        assert_eq!(input.search_query(), Some(""));
    }

    // @req REQ-TUI-017
    #[test]
    fn search_insert_char_ignored_when_not_in_search_mode() {
        let mut input = InputState::default();
        input.search_insert_char('x');
        assert!(!input.in_search_mode());
        assert_eq!(input.search_query(), None);
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
