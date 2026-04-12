//! TUI layout: status line (top), chat log (fill), input (bottom).

use crate::app::status::format_tokens;
use crate::app::{AppPhase, ApprovalDisplayInfo};
use crate::brand;
use crate::messages::{ChatMessage, MessageKind};
use aegis_domain::types::ToolRisk;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

/// Structured status line information.
///
/// Replaces the old `status_text: String` with discrete fields so the renderer
/// can lay out left / center / right sections with phase-appropriate coloring.
#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    /// Model name + mode (left section).
    pub model: String,
    /// Current interaction phase (center section).
    pub phase: AppPhase,
    /// Phase-specific detail text (e.g. thinking animation, "executing tool...").
    pub phase_detail: String,
    /// Input tokens accumulated this session.
    pub input_tokens: u64,
    /// Output tokens accumulated this session.
    pub output_tokens: u64,
}

impl StatusInfo {
    /// Total tokens (input + output) accumulated this session.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Application state for the TUI.
/// Vim mode for display in the hint line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputModeDisplay {
    #[default]
    Insert,
    Normal,
}

/// Spinner animation characters for tool execution.
const SPINNER_CHARS: [char; 4] = ['|', '/', '-', '\\'];

/// Return the spinner character for a given frame index.
fn spinner_char(frame: u8) -> char {
    SPINNER_CHARS[(frame as usize) % SPINNER_CHARS.len()]
}

pub struct AppState {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    /// Cursor byte offset into `input`.
    pub cursor: usize,
    pub status: StatusInfo,
    pub scroll_offset: u16,
    /// Current vim input mode for the hint line.
    pub input_mode: InputModeDisplay,
    /// Platform-detected newline hint text (e.g. "Ctrl+O newline").
    pub newline_hint: String,
    /// Partial streaming response from the LLM, rendered inline below
    /// the last complete message while tokens are arriving.
    pub stream_buffer: String,
    /// When set, a modal overlay is rendered for HITL approval.
    pub approval_display: Option<ApprovalDisplayInfo>,
    /// Current spinner animation frame (cycles 0..3).
    pub spinner_frame: u8,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            cursor: 0,
            status: StatusInfo {
                model: format!("{} v{}", brand::LOGO_COMPACT, brand::VERSION),
                ..Default::default()
            },
            scroll_offset: 0,
            input_mode: InputModeDisplay::Insert,
            newline_hint: "Esc, o new line".to_string(),
            stream_buffer: String::new(),
            approval_display: None,
            spinner_frame: 0,
        }
    }
}

impl AppState {
    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    /// Handle a terminal resize by clamping scroll_offset to valid bounds.
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
    let height = frame.area().height;

    // Input height grows with newlines, capped so chat always has room.
    // count newlines + 1 (Rust's lines() drops trailing empty line after \n)
    let input_lines = (state.input.split('\n').count().max(1)) as u16;
    let max_input = (height / 3).max(1); // never exceed 1/3 of terminal
    let input_height = input_lines.min(max_input);

    // Progressive degradation: show hint line only when terminal is tall enough
    let show_hint = height >= 8;

    let mut constraints = vec![
        Constraint::Length(1),            // Status line
        Constraint::Min(3),               // Chat log
        Constraint::Length(1),            // Separator
        Constraint::Length(input_height), // Input (grows with newlines)
    ];
    if show_hint {
        constraints.push(Constraint::Length(1)); // Hint line
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    render_status_line(frame, chunks[0], state);
    render_chat_log(frame, chunks[1], state);
    render_separator(frame, chunks[2]);
    render_input_line(frame, chunks[3], state);
    if show_hint {
        render_hint_line(frame, chunks[4], state);
    }

    // Render HITL approval modal overlay on top of everything.
    if let Some(ref info) = state.approval_display {
        render_approval_modal(frame, frame.area(), info);
    }
}

fn render_status_line(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let info = &state.status;
    let width = area.width as usize;

    // Phase indicator with color
    let (phase_text, phase_color) = match info.phase {
        AppPhase::Idle => ("", Color::Reset),
        AppPhase::Splash => ("", Color::Reset),
        AppPhase::Streaming => ("STREAMING", Color::Cyan),
        AppPhase::ToolExecuting => ("TOOL", Color::Yellow),
        AppPhase::AwaitingApproval => ("APPROVE?", Color::Rgb(255, 191, 0)),
    };

    // Right section: tokens (only if non-zero)
    let has_tokens = info.input_tokens > 0 || info.output_tokens > 0;
    let right_text = if has_tokens {
        format!(
            "in: {} | out: {}",
            format_tokens(info.input_tokens),
            format_tokens(info.output_tokens),
        )
    } else {
        String::new()
    };
    // Full right section including brackets for width calculation
    let right = if has_tokens {
        format!("[tokens {}]", right_text)
    } else {
        String::new()
    };

    // Build spans based on available width
    let mut spans: Vec<Span> = Vec::new();

    // Left: model name (always shown)
    spans.push(Span::styled(
        &info.model,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));

    // Center: phase (if active and room permits)
    if !phase_text.is_empty() {
        let detail = if info.phase_detail.is_empty() {
            String::new()
        } else {
            format!(" {}", info.phase_detail)
        };
        let phase_section = format!("{phase_text}{detail}");

        // Only show if we have room (model + phase + padding)
        if info.model.len() + phase_section.len() + 6 < width {
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(
                phase_section,
                Style::default().fg(phase_color),
            ));
        }
    }

    // Right: tokens (if room permits), with distinct colors for in vs out
    if !right.is_empty() {
        let used: usize = spans.iter().map(|s| s.content.len()).sum();
        if used + right.len() + 4 < width {
            let padding = width.saturating_sub(used + right.len() + 1);
            spans.push(Span::raw(" ".repeat(padding)));
            spans.push(Span::styled(
                "[tokens in: ",
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                format_tokens(info.input_tokens),
                Style::default().fg(Color::Green),
            ));
            spans.push(Span::styled(
                " | out: ",
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                format_tokens(info.output_tokens),
                Style::default().fg(Color::Rgb(100, 149, 237)),
            ));
            spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
        }
    }

    let status = Paragraph::new(Line::from(spans))
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

    // Render a spinner on the last tool call line when executing.
    if state.status.phase == AppPhase::ToolExecuting {
        // Find the last ToolCall line and prepend a spinner character.
        if let Some(last_tool_idx) = state
            .messages
            .iter()
            .rposition(|m| matches!(m.kind, MessageKind::ToolCall { .. }))
        {
            // Each message produces 2 lines (content + blank), so the
            // content line for message at index i is at position i*2.
            let line_idx = last_tool_idx * 2;
            if line_idx < lines.len() {
                let sc = spinner_char(state.spinner_frame);
                let mut spans = vec![Span::styled(
                    format!("{sc} "),
                    Style::default().fg(Color::Yellow),
                )];
                spans.extend(lines[line_idx].spans.iter().cloned());
                lines[line_idx] = Line::from(spans);
            }
        }
    }

    // Render the streaming buffer as a pending assistant message.
    if !state.stream_buffer.is_empty() {
        lines.push(Line::from(vec![
            Span::raw(&state.stream_buffer),
            Span::styled(
                " ...",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ),
        ]));
        lines.push(Line::from(""));
    }

    // Clamp scroll so we don't scroll past content or when content fits.
    let total_lines = lines.len() as u16;
    let visible_height = area.height;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let clamped_scroll = state.scroll_offset.min(max_scroll);

    let chat = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((clamped_scroll, 0));

    frame.render_widget(chat, area);
}

/// Compute a centered rectangle of approximately the given percentage of the
/// parent area.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render the HITL approval modal as a centered overlay.
fn render_approval_modal(frame: &mut Frame, area: Rect, info: &ApprovalDisplayInfo) {
    let modal_area = centered_rect(60, 40, area);

    // Clear the area behind the modal.
    frame.render_widget(Clear, modal_area);

    let risk_label = match info.risk {
        ToolRisk::StateMutating => "STATE-MUTATING",
        ToolRisk::ReadOnly => "READ-ONLY",
    };
    let risk_color = match info.risk {
        ToolRisk::StateMutating => Color::Red,
        ToolRisk::ReadOnly => Color::Green,
    };

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Tool: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&info.tool_name),
        ]),
        Line::from(vec![
            Span::styled("  Args: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&info.args_summary),
        ]),
        Line::from(vec![
            Span::styled("  Risk: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(risk_label, Style::default().fg(risk_color)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [Y] Approve   [N] Deny",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let block = Block::default()
        .title(" Approval Required ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, modal_area);
}

fn render_separator(frame: &mut Frame, area: Rect) {
    let sep = "-".repeat(area.width as usize);
    let separator = Paragraph::new(Line::from(Span::styled(
        sep,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(separator, area);
}

fn render_input_line(frame: &mut Frame, area: Rect, state: &AppState) {
    let prompt_str = "> ";
    let prompt_len = prompt_str.len();

    // Build lines: first line gets the prompt, continuation lines get padding
    let input_lines: Vec<&str> = if state.input.is_empty() {
        vec![""]
    } else {
        state.input.split('\n').collect()
    };

    let mut lines: Vec<Line> = Vec::new();
    for (i, line_text) in input_lines.iter().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    prompt_str,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(*line_text),
            ]));
        } else {
            // Continuation lines: indent to align with text after prompt
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(prompt_len)),
                Span::raw(*line_text),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);

    // Cursor position: find which line and column the cursor is on
    let text_before_cursor = &state.input[..state.cursor];
    let cursor_line = text_before_cursor.matches('\n').count();
    let cursor_col_in_line = text_before_cursor
        .rsplit('\n')
        .next()
        .unwrap_or(text_before_cursor)
        .chars()
        .count();

    // First line has "> " prefix, continuation lines have "  " padding
    let col_offset = prompt_len as u16 + cursor_col_in_line as u16;
    let row_offset = cursor_line as u16;

    frame.set_cursor_position((area.x + col_offset, area.y + row_offset));
}

fn render_hint_line(frame: &mut Frame, area: Rect, state: &AppState) {
    let mode_label = match state.input_mode {
        InputModeDisplay::Insert => "INSERT",
        InputModeDisplay::Normal => "NORMAL",
    };

    let hints = match state.input_mode {
        InputModeDisplay::Insert => {
            format!("Enter send | {} | Esc vim", state.newline_hint)
        }
        InputModeDisplay::Normal => "i insert | o new line | Esc toggle".to_string(),
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {mode_label} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {hints}"), Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(line), area);
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

        // Status line should show the brand compact logo
        assert!(
            output.contains("aegis"),
            "Status line should contain brand name: {output}"
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

    // @req REQ-TUI-033
    #[test]
    fn status_line_shows_model_name() {
        let mut state = AppState::default();
        state.status.model = "gemini-il5".to_string();
        let output = render_to_string(&state, 80, 20);

        assert!(
            output.contains("gemini-il5"),
            "Should show model name in status: {output}"
        );
    }

    // @req REQ-TUI-033
    #[test]
    fn status_line_shows_phase_when_streaming() {
        let mut state = AppState::default();
        state.status.phase = AppPhase::Streaming;
        state.status.phase_detail = "Analyzing...".to_string();
        let output = render_to_string(&state, 80, 20);

        assert!(
            output.contains("STREAMING"),
            "Should show STREAMING phase: {output}"
        );
        assert!(
            output.contains("Analyzing"),
            "Should show phase detail: {output}"
        );
    }

    // @req REQ-TUI-033
    #[test]
    fn status_line_shows_tokens_when_nonzero() {
        let mut state = AppState::default();
        state.status.input_tokens = 1500;
        state.status.output_tokens = 320;
        let output = render_to_string(&state, 80, 20);

        assert!(
            output.contains("1.5k"),
            "Should show formatted input tokens: {output}"
        );
        assert!(
            output.contains("320"),
            "Should show output tokens: {output}"
        );
    }

    // @req REQ-TUI-033
    #[test]
    fn status_line_hides_tokens_when_zero() {
        let state = AppState::default();
        let output = render_to_string(&state, 80, 20);
        assert!(
            !output.contains("0in"),
            "Should not show zero tokens: {output}"
        );
    }

    // @req REQ-TUI-033
    #[test]
    fn status_line_degrades_on_narrow_terminal() {
        let mut state = AppState::default();
        state.status.model = "long-model-name".to_string();
        state.status.phase = AppPhase::Streaming;
        state.status.phase_detail = "detail".to_string();
        state.status.input_tokens = 999;
        state.status.output_tokens = 888;
        // Very narrow -- should still render without panic
        let output = render_to_string(&state, 30, 10);
        assert!(
            output.contains("long-model-name"),
            "Model should always show: {output}"
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

    // @req REQ-TUI-032
    #[test]
    fn layout_renders_streaming_buffer_inline() {
        let state = AppState {
            stream_buffer: "partial response".to_string(),
            ..Default::default()
        };
        let output = render_to_string(&state, 80, 20);
        assert!(
            output.contains("partial response"),
            "Should show streaming buffer content: {output}"
        );
        assert!(
            output.contains("..."),
            "Should show streaming indicator: {output}"
        );
    }

    // @req REQ-TUI-032
    #[test]
    fn layout_does_not_render_empty_streaming_buffer() {
        let state = AppState::default();
        let output = render_to_string(&state, 60, 20);
        // The "..." streaming indicator should NOT appear when buffer is empty.
        // Count occurrences of "..." -- there should be none from the streaming
        // buffer (the input prompt ">" is present but "..." is not).
        assert!(
            !output.contains("..."),
            "Empty stream buffer should not render indicator: {output}"
        );
    }

    // @req REQ-TUI-032
    #[test]
    fn layout_renders_streaming_buffer_after_messages() {
        let mut state = AppState::default();
        state.push_message(ChatMessage::user("hello"));
        state.stream_buffer = "responding".to_string();
        let output = render_to_string(&state, 80, 20);
        assert!(output.contains("You:"));
        assert!(output.contains("responding"));
    }

    // @req REQ-TUI-029
    #[test]
    fn layout_renders_approval_modal_overlay() {
        let state = AppState {
            approval_display: Some(ApprovalDisplayInfo {
                tool_name: "write_file".to_string(),
                args_summary: "src/main.rs".to_string(),
                risk: ToolRisk::StateMutating,
            }),
            ..Default::default()
        };
        let output = render_to_string(&state, 80, 25);
        assert!(
            output.contains("Approval Required"),
            "Should show modal title: {output}"
        );
        assert!(
            output.contains("write_file"),
            "Should show tool name: {output}"
        );
        assert!(output.contains("src/main.rs"), "Should show args: {output}");
        assert!(
            output.contains("STATE-MUTATING"),
            "Should show risk level: {output}"
        );
    }

    // @req REQ-TUI-029
    #[test]
    fn layout_does_not_render_modal_when_no_approval() {
        let state = AppState::default();
        let output = render_to_string(&state, 80, 25);
        assert!(
            !output.contains("Approval Required"),
            "Should not show modal when no approval pending: {output}"
        );
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

    // @req REQ-TUI-037
    #[test]
    fn input_prompt_inline_with_text() {
        let state = AppState {
            input: "hello".to_string(),
            ..Default::default()
        };
        let output = render_to_string(&state, 60, 20);
        // The > prompt and text should appear on the same line
        let has_inline = output.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.contains('>') && trimmed.contains("hello")
        });
        assert!(
            has_inline,
            "Prompt > and text should be on same line: {output}"
        );
    }

    // @req REQ-TUI-037
    #[test]
    fn input_prompt_visible_when_empty() {
        let state = AppState::default();
        let output = render_to_string(&state, 60, 20);
        // Should still show the > prompt even with no input text
        let has_prompt = output.lines().any(|line| line.contains('>'));
        assert!(has_prompt, "Should show > prompt when empty: {output}");
    }

    // @req REQ-TUI-038
    #[test]
    fn input_no_box_drawing_border() {
        let state = AppState::default();
        let output = render_to_string(&state, 60, 20);
        // Box-drawing characters from Borders::TOP should not appear
        let box_chars = [
            '\u{2500}', '\u{2502}', '\u{250c}', '\u{2510}', '\u{2514}', '\u{2518}',
        ];
        for ch in &box_chars {
            assert!(
                !output.contains(*ch),
                "Should not contain box-drawing char U+{:04X}: {output}",
                *ch as u32,
            );
        }
    }

    // @req REQ-TUI-040
    #[test]
    fn hint_line_shows_insert_mode() {
        let state = AppState::default();
        let output = render_to_string(&state, 80, 20);
        assert!(
            output.contains("INSERT"),
            "Should show INSERT mode hint: {output}"
        );
    }

    // @req REQ-TUI-040
    #[test]
    fn hint_line_shows_normal_mode() {
        let state = AppState {
            input_mode: InputModeDisplay::Normal,
            ..Default::default()
        };
        let output = render_to_string(&state, 80, 20);
        assert!(
            output.contains("NORMAL"),
            "Should show NORMAL mode hint: {output}"
        );
    }

    // @req REQ-TUI-040
    #[test]
    fn hint_line_shows_key_hints() {
        let state = AppState::default();
        let output = render_to_string(&state, 80, 20);
        assert!(
            output.contains("Enter"),
            "Should show Enter key hint: {output}"
        );
    }

    // @req REQ-TUI-040
    #[test]
    fn hint_line_hidden_on_short_terminal() {
        let state = AppState::default();
        // With only 5 rows: status(1) + chat(min 3) + sep(1) + input(1) = 6 minimum
        // No room for hint line at height 5
        let output = render_to_string(&state, 60, 5);
        // Should not panic and should still render
        assert!(!output.is_empty());
    }

    // @req REQ-TUI-019
    #[test]
    fn status_line_renders_formatted_tokens() {
        let mut state = AppState::default();
        state.status.input_tokens = 1500;
        state.status.output_tokens = 3400;
        let output = render_to_string(&state, 80, 20);

        assert!(
            output.contains("1.5k"),
            "Should show formatted input tokens as 1.5k: {output}"
        );
        assert!(
            output.contains("3.4k"),
            "Should show formatted output tokens as 3.4k: {output}"
        );
        assert!(
            output.contains("tokens"),
            "Should show tokens label: {output}"
        );
    }

    // @req REQ-TUI-019
    #[test]
    fn status_line_shows_tokens_during_streaming() {
        let mut state = AppState::default();
        state.status.phase = AppPhase::Streaming;
        state.status.phase_detail = "thinking".to_string();
        state.status.input_tokens = 2500;
        state.status.output_tokens = 800;
        let output = render_to_string(&state, 100, 20);

        assert!(
            output.contains("2.5k"),
            "Should show input tokens during streaming: {output}"
        );
        assert!(
            output.contains("800"),
            "Should show output tokens during streaming: {output}"
        );
    }

    // @req REQ-TUI-019
    #[test]
    fn status_info_total_tokens() {
        let info = StatusInfo {
            input_tokens: 1500,
            output_tokens: 500,
            ..Default::default()
        };
        assert_eq!(info.total_tokens(), 2000);
    }

    // @req REQ-TUI-019
    #[test]
    fn status_info_total_tokens_zero() {
        let info = StatusInfo::default();
        assert_eq!(info.total_tokens(), 0);
    }

    // @req REQ-TUI-016
    #[test]
    fn spinner_char_cycles_through_frames() {
        assert_eq!(spinner_char(0), '|');
        assert_eq!(spinner_char(1), '/');
        assert_eq!(spinner_char(2), '-');
        assert_eq!(spinner_char(3), '\\');
        assert_eq!(spinner_char(4), '|'); // wraps
    }

    // @req REQ-TUI-016
    #[test]
    fn spinner_renders_during_tool_executing() {
        let mut state = AppState::default();
        state.status.phase = AppPhase::ToolExecuting;
        state.push_message(ChatMessage::tool_call("read_file", "src/main.rs"));
        state.spinner_frame = 1; // '/'
        let output = render_to_string(&state, 80, 20);
        assert!(
            output.contains('/'),
            "Should show spinner character during tool execution: {output}"
        );
    }

    // @req REQ-TUI-016
    #[test]
    fn no_spinner_when_idle() {
        let mut state = AppState::default();
        state.status.phase = AppPhase::Idle;
        state.push_message(ChatMessage::tool_call("read_file", "src/main.rs"));
        state.spinner_frame = 1;
        let output = render_to_string(&state, 80, 20);
        // The tool call line should NOT have a spinner prefix when idle.
        // Look for the tool call without the spinner prefix pattern.
        let has_spinner_prefix = output.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("/ ")
                && (trimmed.contains("read_file") || trimmed.contains("src/main.rs"))
        });
        assert!(
            !has_spinner_prefix,
            "Should not show spinner when idle: {output}"
        );
    }

    // @req REQ-TUI-038
    #[test]
    fn input_has_separator_line() {
        let state = AppState::default();
        let output = render_to_string(&state, 60, 20);
        // Should have a dash-based separator line
        assert!(
            output.contains("----------"),
            "Should have a dash separator above input: {output}"
        );
    }
}
