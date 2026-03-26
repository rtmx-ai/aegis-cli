//! Simple markdown rendering for chat messages.
//!
//! Converts a subset of Markdown into styled ratatui `Line` objects:
//! - `**bold**` -> bold style
//! - `` `code` `` -> gray background style
//! - `# heading` -> bold + underline style
//! - Plain text passes through unstyled.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a markdown string into styled ratatui Lines.
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    if text.is_empty() {
        return vec![Line::from(Span::raw(String::new()))];
    }
    text.lines().map(render_line).collect()
}

fn render_line(line: &str) -> Line<'static> {
    // Check for heading
    if let Some(heading_text) = line.strip_prefix("# ") {
        return Line::from(Span::styled(
            heading_text.to_string(),
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        ));
    }

    // Parse inline bold and code spans
    let spans = parse_inline(line);
    Line::from(spans)
}

enum Marker {
    Bold(usize),
    Code(usize),
}

fn find_next_marker(text: &str) -> Option<Marker> {
    let bold_pos = text.find("**");
    let code_pos = text.find('`');
    match (bold_pos, code_pos) {
        (Some(b), Some(c)) if b <= c => Some(Marker::Bold(b)),
        (Some(_), Some(c)) => Some(Marker::Code(c)),
        (Some(b), None) => Some(Marker::Bold(b)),
        (None, Some(c)) => Some(Marker::Code(c)),
        (None, None) => None,
    }
}

fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        match find_next_marker(remaining) {
            None => {
                spans.push(Span::raw(remaining.to_string()));
                return spans;
            }
            Some(Marker::Bold(pos)) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let after_open = &remaining[pos + 2..];
                if let Some(close) = after_open.find("**") {
                    spans.push(Span::styled(
                        after_open[..close].to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                    remaining = &after_open[close + 2..];
                } else {
                    spans.push(Span::raw(remaining[pos..].to_string()));
                    return spans;
                }
            }
            Some(Marker::Code(pos)) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let after_open = &remaining[pos + 1..];
                if let Some(close) = after_open.find('`') {
                    spans.push(Span::styled(
                        after_open[..close].to_string(),
                        Style::default().bg(Color::DarkGray),
                    ));
                    remaining = &after_open[close + 1..];
                } else {
                    spans.push(Span::raw(remaining[pos..].to_string()));
                    return spans;
                }
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-TUI-002
    #[test]
    fn plain_text_passes_through() {
        let lines = render_markdown("Hello world");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].content, "Hello world");
        assert_eq!(lines[0].spans[0].style, Style::default());
    }

    // @req REQ-TUI-002
    #[test]
    fn bold_text_rendered_bold() {
        let lines = render_markdown("This is **bold** text");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[0].content, "This is ");
        assert_eq!(lines[0].spans[1].content, "bold");
        assert!(
            lines[0].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[0].spans[2].content, " text");
    }

    // @req REQ-TUI-002
    #[test]
    fn code_span_rendered_with_gray_background() {
        let lines = render_markdown("Run `cargo test` now");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[0].content, "Run ");
        assert_eq!(lines[0].spans[1].content, "cargo test");
        assert_eq!(
            lines[0].spans[1].style,
            Style::default().bg(Color::DarkGray)
        );
        assert_eq!(lines[0].spans[2].content, " now");
    }

    // @req REQ-TUI-002
    #[test]
    fn heading_rendered_bold_underline() {
        let lines = render_markdown("# My Heading");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].content, "My Heading");
        let style = lines[0].spans[0].style;
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::UNDERLINED));
    }

    // @req REQ-TUI-002
    #[test]
    fn multiline_text_produces_multiple_lines() {
        let lines = render_markdown("Line one\nLine two\nLine three");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "Line one");
        assert_eq!(lines[1].spans[0].content, "Line two");
        assert_eq!(lines[2].spans[0].content, "Line three");
    }

    // @req REQ-TUI-002
    #[test]
    fn mixed_bold_and_code() {
        let lines = render_markdown("Use **bold** and `code` together");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 5);
        assert_eq!(lines[0].spans[0].content, "Use ");
        assert_eq!(lines[0].spans[1].content, "bold");
        assert!(
            lines[0].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[0].spans[2].content, " and ");
        assert_eq!(lines[0].spans[3].content, "code");
        assert_eq!(
            lines[0].spans[3].style,
            Style::default().bg(Color::DarkGray)
        );
        assert_eq!(lines[0].spans[4].content, " together");
    }

    // @req REQ-TUI-002
    #[test]
    fn unclosed_bold_treated_as_plain() {
        let lines = render_markdown("This is **unclosed");
        assert_eq!(lines.len(), 1);
        // Should contain the raw text since ** is not closed
        let full_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full_text, "This is **unclosed");
    }

    // @req REQ-TUI-002
    #[test]
    fn unclosed_code_treated_as_plain() {
        let lines = render_markdown("Run `unclosed");
        assert_eq!(lines.len(), 1);
        let full_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full_text, "Run `unclosed");
    }

    // @req REQ-TUI-002
    #[test]
    fn empty_input_produces_single_empty_line() {
        let lines = render_markdown("");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, "");
    }

    // @req REQ-TUI-002
    #[test]
    fn heading_among_other_lines() {
        let lines = render_markdown("# Title\nSome body text");
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[1].spans[0].style, Style::default());
    }
}
