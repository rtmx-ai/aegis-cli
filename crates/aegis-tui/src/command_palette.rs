//! Slash command palette: floating autocomplete dropdown.
//!
//! Shows available commands when the user types `/` in the input field.
//! Supports prefix filtering, up/down navigation, and Tab completion.

/// A slash command entry for the palette.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub usage: Option<String>,
}

/// Snapshot of palette state for rendering.
#[derive(Debug, Clone)]
pub struct CommandPaletteView {
    pub entries: Vec<CommandEntry>,
    pub selected: usize,
}

/// The command palette state.
pub struct CommandPalette {
    all_commands: Vec<CommandEntry>,
    filtered: Vec<CommandEntry>,
    selected: usize,
    pub is_visible: bool,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        let all_commands = vec![
            cmd("/help", "Show available commands and usage"),
            cmd_with_usage(
                "/connect",
                "Connect to an LLM provider",
                "<local|vertex|bedrock|azure>",
            ),
            cmd_with_usage("/model", "Switch or display current model", "<name>"),
            cmd_with_usage("/add", "Add file to conversation context", "<path>"),
            cmd_with_usage("/drop", "Remove file from context", "<path>"),
            cmd("/context", "Show current context files"),
            cmd_with_usage("/search", "Search conversation history", "<query>"),
            cmd_with_usage(
                "/infra",
                "Infrastructure plugin operations",
                "<list|status|up|preview|destroy>",
            ),
            cmd("/doctor", "Run health and connectivity checks"),
            cmd("/copy", "Copy last code block to clipboard"),
            cmd("/undo", "Revert most recent approved write"),
            cmd("/clear", "Clear conversation history"),
            cmd("/quit", "Exit aegis"),
        ];
        Self {
            filtered: all_commands.clone(),
            all_commands,
            selected: 0,
            is_visible: false,
        }
    }

    pub fn show(&mut self) {
        self.is_visible = true;
        self.selected = 0;
        self.filtered = self.all_commands.clone();
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    pub fn filter(&mut self, prefix: &str) {
        let p = prefix.to_lowercase();
        self.filtered = self
            .all_commands
            .iter()
            .filter(|c| c.name.to_lowercase().starts_with(&p))
            .cloned()
            .collect();
        self.selected = 0;
    }

    pub fn next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    pub fn selected_entry(&self) -> Option<&CommandEntry> {
        self.filtered.get(self.selected)
    }

    pub fn selected_command(&self) -> Option<&str> {
        self.filtered.get(self.selected).map(|e| e.name.as_str())
    }

    pub fn view(&self) -> Option<CommandPaletteView> {
        if !self.is_visible || self.filtered.is_empty() {
            return None;
        }
        Some(CommandPaletteView {
            entries: self.filtered.clone(),
            selected: self.selected,
        })
    }
}

fn cmd(name: &str, desc: &str) -> CommandEntry {
    CommandEntry {
        name: name.into(),
        description: desc.into(),
        usage: None,
    }
}

fn cmd_with_usage(name: &str, desc: &str, usage: &str) -> CommandEntry {
    CommandEntry {
        name: name.into(),
        description: desc.into(),
        usage: Some(usage.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-040
    #[test]
    fn new_has_all_commands() {
        let p = CommandPalette::new();
        assert!(p.all_commands.len() >= 11);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn filter_by_prefix() {
        let mut p = CommandPalette::new();
        p.filter("/c");
        let names: Vec<_> = p.filtered.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"/connect"));
        assert!(names.contains(&"/context"));
        assert!(names.contains(&"/clear"));
        assert!(!names.contains(&"/help"));
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn filter_no_match() {
        let mut p = CommandPalette::new();
        p.filter("/xyz");
        assert!(p.filtered.is_empty());
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn next_wraps_around() {
        let mut p = CommandPalette::new();
        p.filter("/");
        let len = p.filtered.len();
        for _ in 0..len {
            p.next();
        }
        assert_eq!(p.selected, 0);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn prev_wraps_around() {
        let mut p = CommandPalette::new();
        p.filter("/");
        p.prev();
        assert_eq!(p.selected, p.filtered.len() - 1);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn selected_command_returns_name() {
        let p = CommandPalette::new();
        assert_eq!(p.selected_command(), Some("/help"));
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn show_sets_visible() {
        let mut p = CommandPalette::new();
        p.show();
        assert!(p.is_visible);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn hide_clears_visible() {
        let mut p = CommandPalette::new();
        p.show();
        p.hide();
        assert!(!p.is_visible);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn view_returns_none_when_hidden() {
        let p = CommandPalette::new();
        assert!(p.view().is_none());
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn view_returns_some_when_visible() {
        let mut p = CommandPalette::new();
        p.show();
        let v = p.view();
        assert!(v.is_some());
        assert_eq!(v.unwrap().entries.len(), p.all_commands.len());
    }
}
