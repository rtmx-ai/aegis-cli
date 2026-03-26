//! TUI layout: status line (top), chat log (fill), input (bottom).

use crate::messages::{ChatMessage, MessageKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Application state for the TUI.
pub struct AppState {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub status_text: String,
    pub scroll_offset: u16,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            status_text: "aegis v0.1.0".to_string(),
            scroll_offset: 0,
        }
    }
}

impl AppState {
    pub fn with_status(mut self, status: &str) -> Self {
        self.status_text = status.to_string();
        self
    }

    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    /// Handle a terminal resize by clamping scroll_offset to valid bounds.
    ///
    /// After a resize, the visible area may have changed. If the current
    /// scroll_offset would place us past the end of the content, clamp it
    /// so the last line of content is visible.
    pub fn resize(&mut self, total_lines: u16, visible_height: u16) {
        if visible_height >= total_lines {
            self.scroll_offset = 0;
        } else {
            let max_scroll = total_lines - visible_height;
            if self.scroll_offset > max_scroll {
                self.scroll_offset = max_scroll;
            }
        }
    }
}

/// Render the full application layout.
pub fn render(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Status line
            Constraint::Min(3),    // Chat log
            Constraint::Length(3), // Input
        ])
        .split(frame.area());

    render_status_line(frame, chunks[0], state);
    render_chat_log(frame, chunks[1], state);
    render_input(frame, chunks[2], state);
}

fn render_status_line(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let status = Paragraph::new(Line::from(vec![Span::styled(
        &state.status_text,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]))
    .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(status, area);
}

fn render_chat_log(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &state.messages {
        match &msg.kind {
            MessageKind::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "You: ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&msg.content),
                ]));
            }
            MessageKind::Assistant => {
                lines.push(Line::from(vec![Span::raw(&msg.content)]));
            }
            MessageKind::ToolCall { tool_name } => {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  > {tool_name}: "),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(&msg.content, Style::default().fg(Color::DarkGray)),
                ]));
            }
            MessageKind::ToolResult => {
                lines.push(Line::from(vec![Span::styled(
                    &msg.content,
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            MessageKind::Error => {
                lines.push(Line::from(vec![Span::styled(
                    &msg.content,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )]));
            }
            MessageKind::System => {
                lines.push(Line::from(vec![Span::styled(
                    &msg.content,
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )]));
            }
        }
        lines.push(Line::from(""));
    }

    let chat = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset, 0));

    frame.render_widget(chat, area);
}

fn render_input(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let input = Paragraph::new(state.input.as_str())
        .block(Block::default().borders(Borders::TOP).title(" > "))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_to_string(state: &AppState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, state)).unwrap();
        terminal.backend().to_string()
    }

    // @req REQ-TUI-001
    #[test]
    fn layout_renders_three_sections() {
        let state = AppState::default();
        let output = render_to_string(&state, 60, 20);

        // Status line should be visible at top
        assert!(
            output.contains("aegis v0.1.0"),
            "Status line should contain version: {output}"
        );
        // Input area should have the border
        assert!(output.contains(">"), "Input area should have prompt marker");
    }

    // @req REQ-TUI-001
    #[test]
    fn layout_renders_user_message() {
        let mut state = AppState::default();
        state.push_message(ChatMessage::user("Hello world"));
        let output = render_to_string(&state, 60, 20);

        assert!(
            output.contains("You:"),
            "Should show 'You:' prefix: {output}"
        );
        assert!(
            output.contains("Hello world"),
            "Should show message content: {output}"
        );
    }

    // @req REQ-TUI-001
    #[test]
    fn layout_renders_assistant_message() {
        let mut state = AppState::default();
        state.push_message(ChatMessage::assistant("I can help with that."));
        let output = render_to_string(&state, 60, 20);

        assert!(
            output.contains("I can help"),
            "Should show assistant text: {output}"
        );
    }

    // @req REQ-TUI-005
    #[test]
    fn layout_renders_tool_call() {
        let mut state = AppState::default();
        state.push_message(ChatMessage::tool_call("read_file", "src/main.rs"));
        let output = render_to_string(&state, 60, 20);

        assert!(
            output.contains("read_file"),
            "Should show tool name: {output}"
        );
        assert!(
            output.contains("src/main.rs"),
            "Should show tool detail: {output}"
        );
    }

    // @req REQ-TUI-015
    #[test]
    fn layout_renders_error_message() {
        let mut state = AppState::default();
        state.push_message(ChatMessage::error("Connection failed"));
        let output = render_to_string(&state, 60, 20);

        assert!(
            output.contains("Connection failed"),
            "Should show error: {output}"
        );
    }

    // @req REQ-TUI-001
    #[test]
    fn layout_renders_custom_status() {
        let state = AppState::default().with_status("IL5 Assured Workloads (us-central1)");
        let output = render_to_string(&state, 80, 20);

        assert!(
            output.contains("IL5 Assured Workloads"),
            "Should show custom status: {output}"
        );
    }

    // @req REQ-TUI-001
    #[test]
    fn layout_renders_input_text() {
        let state = AppState {
            input: "Fix the bug".to_string(),
            ..Default::default()
        };
        let output = render_to_string(&state, 60, 20);

        assert!(
            output.contains("Fix the bug"),
            "Should show input text: {output}"
        );
    }

    // @req REQ-TUI-001
    #[test]
    fn layout_handles_empty_state() {
        let state = AppState::default();
        let output = render_to_string(&state, 40, 10);

        // Should render without panic even at small size
        assert!(!output.is_empty());
    }

    // @req REQ-TUI-001
    #[test]
    fn layout_renders_multiple_messages() {
        let mut state = AppState::default();
        state.push_message(ChatMessage::user("Explain main.rs"));
        state.push_message(ChatMessage::tool_call("read_file", "src/main.rs (4.2KB)"));
        state.push_message(ChatMessage::assistant(
            "The main function initializes the app.",
        ));
        let output = render_to_string(&state, 80, 25);

        assert!(output.contains("You:"));
        assert!(output.contains("read_file"));
        assert!(output.contains("main function"));
    }

    fn state_with_scroll(offset: u16) -> AppState {
        AppState {
            scroll_offset: offset,
            ..Default::default()
        }
    }

    // @req REQ-TUI-007
    #[test]
    fn resize_clamps_scroll_offset_when_exceeds_max() {
        let mut state = state_with_scroll(50);
        // total_lines=30, visible_height=10 -> max_scroll=20
        state.resize(30, 10);
        assert_eq!(state.scroll_offset, 20);
    }

    // @req REQ-TUI-007
    #[test]
    fn resize_keeps_scroll_offset_when_within_bounds() {
        let mut state = state_with_scroll(5);
        // total_lines=30, visible_height=10 -> max_scroll=20, offset 5 is fine
        state.resize(30, 10);
        assert_eq!(state.scroll_offset, 5);
    }

    // @req REQ-TUI-007
    #[test]
    fn resize_resets_to_zero_when_content_fits() {
        let mut state = state_with_scroll(10);
        // visible_height >= total_lines, everything fits
        state.resize(5, 20);
        assert_eq!(state.scroll_offset, 0);
    }

    // @req REQ-TUI-007
    #[test]
    fn resize_handles_equal_height_and_lines() {
        let mut state = state_with_scroll(3);
        // total_lines == visible_height -> all content visible
        state.resize(10, 10);
        assert_eq!(state.scroll_offset, 0);
    }

    // @req REQ-TUI-007
    #[test]
    fn resize_at_exact_max_scroll_stays() {
        let mut state = state_with_scroll(20);
        // total_lines=30, visible_height=10 -> max_scroll=20, exactly at boundary
        state.resize(30, 10);
        assert_eq!(state.scroll_offset, 20);
    }

    // @req REQ-TUI-007
    #[test]
    fn resize_zero_total_lines_resets_offset() {
        let mut state = state_with_scroll(5);
        state.resize(0, 10);
        assert_eq!(state.scroll_offset, 0);
    }
}
