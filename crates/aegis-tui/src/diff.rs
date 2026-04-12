//! Inline unified diff rendering for proposed file changes.
//!
//! Parses unified diff format and returns styled ratatui `Line` objects:
//! - `+` added lines -> green foreground
//! - `-` removed lines -> red foreground
//! - `@@` hunk headers -> cyan foreground
//! - `---` / `+++` file headers -> bold
//! - Context lines (no prefix) -> default style

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a unified diff string into styled ratatui Lines.
///
/// Each line of the diff is classified by its prefix and styled accordingly:
/// - File headers (`---`, `+++`) are rendered bold.
/// - Hunk headers (`@@`) are rendered in cyan.
/// - Added lines (`+`) are rendered in green.
/// - Removed lines (`-`) are rendered in red.
/// - Context lines are rendered with the default style.
pub fn render_diff(diff_text: &str) -> Vec<Line<'static>> {
    if diff_text.is_empty() {
        return vec![Line::from(Span::raw(String::new()))];
    }
    diff_text.lines().map(style_diff_line).collect()
}

/// Heuristic to detect whether a text block looks like a unified diff.
///
/// Returns `true` if the text contains at least one hunk header (`@@ ... @@`)
/// and at least one file header line (`---` or `+++`).
pub fn is_diff(text: &str) -> bool {
    let has_hunk_header = text.lines().any(|l| l.starts_with("@@"));
    let has_file_header = text
        .lines()
        .any(|l| l.starts_with("--- ") || l.starts_with("+++ "));
    has_hunk_header && has_file_header
}

fn style_diff_line(line: &str) -> Line<'static> {
    let owned = line.to_string();

    if owned.starts_with("--- ") || owned.starts_with("+++ ") {
        // File header: bold
        Line::from(Span::styled(
            owned,
            Style::default().add_modifier(Modifier::BOLD),
        ))
    } else if owned.starts_with("@@") {
        // Hunk header: cyan
        Line::from(Span::styled(owned, Style::default().fg(Color::Cyan)))
    } else if owned.starts_with('+') {
        // Added line: green
        Line::from(Span::styled(owned, Style::default().fg(Color::Green)))
    } else if owned.starts_with('-') {
        // Removed line: red
        Line::from(Span::styled(owned, Style::default().fg(Color::Red)))
    } else {
        // Context line: default style
        Line::from(Span::raw(owned))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-003
    #[test]
    fn added_lines_get_green_style() {
        let lines = render_diff("+added line");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].style, Style::default().fg(Color::Green));
        assert_eq!(lines[0].spans[0].content, "+added line");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn removed_lines_get_red_style() {
        let lines = render_diff("-removed line");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].style, Style::default().fg(Color::Red));
        assert_eq!(lines[0].spans[0].content, "-removed line");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn hunk_headers_get_cyan_style() {
        let lines = render_diff("@@ -1,3 +1,4 @@");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].style, Style::default().fg(Color::Cyan));
        assert_eq!(lines[0].spans[0].content, "@@ -1,3 +1,4 @@");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn file_headers_get_bold_style() {
        let diff = "--- a/src/main.rs\n+++ b/src/main.rs";
        let lines = render_diff(diff);
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            lines[1].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[0].spans[0].content, "--- a/src/main.rs");
        assert_eq!(lines[1].spans[0].content, "+++ b/src/main.rs");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn context_lines_are_unstyled() {
        let lines = render_diff(" context line here");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].style, Style::default());
        assert_eq!(lines[0].spans[0].content, " context line here");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn is_diff_detects_unified_diff() {
        let diff = "--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,4 @@\n context\n+added";
        assert!(is_diff(diff));
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn is_diff_rejects_plain_text() {
        assert!(!is_diff("Hello world"));
        assert!(!is_diff("Just some lines\nof text"));
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn is_diff_requires_both_hunk_and_file_header() {
        // Only hunk header, no file header
        assert!(!is_diff("@@ -1,3 +1,4 @@\n+added"));
        // Only file header, no hunk header
        assert!(!is_diff("--- a/file.rs\n+++ b/file.rs\n+added"));
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn empty_input_produces_single_empty_line() {
        let lines = render_diff("");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, "");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn full_diff_renders_all_line_types() {
        let diff = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 use std::io;
-fn old_function() {}
+fn new_function() {}
+fn extra() {}";
        let lines = render_diff(diff);
        assert_eq!(lines.len(), 7);
        // File headers: bold
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            lines[1].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        // Hunk header: cyan
        assert_eq!(lines[2].spans[0].style.fg, Some(Color::Cyan));
        // Context line: default
        assert_eq!(lines[3].spans[0].style, Style::default());
        // Removed line: red
        assert_eq!(lines[4].spans[0].style.fg, Some(Color::Red));
        // Added lines: green
        assert_eq!(lines[5].spans[0].style.fg, Some(Color::Green));
        assert_eq!(lines[6].spans[0].style.fg, Some(Color::Green));
    }
}
