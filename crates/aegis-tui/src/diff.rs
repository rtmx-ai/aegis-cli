//! Inline unified diff rendering for proposed file changes.
//!
//! Parses unified diff format and returns styled ratatui `Line` objects
//! using colors from the active [`Theme`]:
//! - `+` added lines -> `theme.success`
//! - `-` removed lines -> `theme.error`
//! - `@@` hunk headers -> `theme.diff_hunk`
//! - `---` / `+++` file headers -> bold `theme.fg`
//! - Context lines (no prefix) -> `theme.fg`

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// Render a unified diff string into styled ratatui Lines.
///
/// Each line of the diff is classified by its prefix and styled using
/// the provided theme:
/// - File headers (`---`, `+++`) are rendered bold with `theme.fg`.
/// - Hunk headers (`@@`) are rendered with `theme.diff_hunk`.
/// - Added lines (`+`) are rendered with `theme.success`.
/// - Removed lines (`-`) are rendered with `theme.error`.
/// - Context lines are rendered with `theme.fg`.
pub fn render_diff(diff_text: &str, theme: &Theme) -> Vec<Line<'static>> {
    if diff_text.is_empty() {
        return vec![Line::from(Span::raw(String::new()))];
    }
    diff_text
        .lines()
        .map(|line| style_diff_line(line, theme))
        .collect()
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

fn style_diff_line(line: &str, theme: &Theme) -> Line<'static> {
    let owned = line.to_string();

    if owned.starts_with("--- ") || owned.starts_with("+++ ") {
        // File header: bold with theme fg
        Line::from(Span::styled(
            owned,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
    } else if owned.starts_with("@@") {
        // Hunk header: theme diff_hunk color
        Line::from(Span::styled(owned, Style::default().fg(theme.diff_hunk)))
    } else if owned.starts_with('+') {
        // Added line: theme success color
        Line::from(Span::styled(owned, Style::default().fg(theme.success)))
    } else if owned.starts_with('-') {
        // Removed line: theme error color
        Line::from(Span::styled(owned, Style::default().fg(theme.error)))
    } else {
        // Context line: theme fg color
        Line::from(Span::styled(owned, Style::default().fg(theme.fg)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{DARK_THEME, LIGHT_THEME};

    // rtmx:req REQ-TUI-003
    #[test]
    fn added_lines_get_success_style() {
        let lines = render_diff("+added line", &DARK_THEME);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].spans[0].style,
            Style::default().fg(DARK_THEME.success)
        );
        assert_eq!(lines[0].spans[0].content, "+added line");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn removed_lines_get_error_style() {
        let lines = render_diff("-removed line", &DARK_THEME);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].spans[0].style,
            Style::default().fg(DARK_THEME.error)
        );
        assert_eq!(lines[0].spans[0].content, "-removed line");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn hunk_headers_get_diff_hunk_style() {
        let lines = render_diff("@@ -1,3 +1,4 @@", &DARK_THEME);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].spans[0].style,
            Style::default().fg(DARK_THEME.diff_hunk)
        );
        assert_eq!(lines[0].spans[0].content, "@@ -1,3 +1,4 @@");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn file_headers_get_bold_fg_style() {
        let diff = "--- a/src/main.rs\n+++ b/src/main.rs";
        let lines = render_diff(diff, &DARK_THEME);
        assert_eq!(lines.len(), 2);
        let expected = Style::default()
            .fg(DARK_THEME.fg)
            .add_modifier(Modifier::BOLD);
        assert_eq!(lines[0].spans[0].style, expected);
        assert_eq!(lines[1].spans[0].style, expected);
        assert_eq!(lines[0].spans[0].content, "--- a/src/main.rs");
        assert_eq!(lines[1].spans[0].content, "+++ b/src/main.rs");
    }

    // rtmx:req REQ-TUI-003
    #[test]
    fn context_lines_get_fg_style() {
        let lines = render_diff(" context line here", &DARK_THEME);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].style, Style::default().fg(DARK_THEME.fg));
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
        let lines = render_diff("", &DARK_THEME);
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
        let lines = render_diff(diff, &DARK_THEME);
        assert_eq!(lines.len(), 7);
        // File headers: bold fg
        let bold_fg = Style::default()
            .fg(DARK_THEME.fg)
            .add_modifier(Modifier::BOLD);
        assert_eq!(lines[0].spans[0].style, bold_fg);
        assert_eq!(lines[1].spans[0].style, bold_fg);
        // Hunk header: diff_hunk
        assert_eq!(lines[2].spans[0].style.fg, Some(DARK_THEME.diff_hunk));
        // Context line: fg
        assert_eq!(lines[3].spans[0].style.fg, Some(DARK_THEME.fg));
        // Removed line: error
        assert_eq!(lines[4].spans[0].style.fg, Some(DARK_THEME.error));
        // Added lines: success
        assert_eq!(lines[5].spans[0].style.fg, Some(DARK_THEME.success));
        assert_eq!(lines[6].spans[0].style.fg, Some(DARK_THEME.success));
    }

    // rtmx:req REQ-TUI-099
    #[test]
    fn test_diff_uses_theme_colors() {
        // Verify diff rendering uses theme colors instead of hardcoded values.
        // Render the same diff with both themes and confirm the colors differ.
        let diff = "+added line\n-removed line\n@@ -1,3 +1,4 @@\n";
        let dark_lines = render_diff(diff, &DARK_THEME);
        let light_lines = render_diff(diff, &LIGHT_THEME);

        // Added line uses theme.success (differs between themes)
        assert_eq!(dark_lines[0].spans[0].style.fg, Some(DARK_THEME.success));
        assert_eq!(light_lines[0].spans[0].style.fg, Some(LIGHT_THEME.success));
        assert_ne!(
            dark_lines[0].spans[0].style.fg,
            light_lines[0].spans[0].style.fg,
        );

        // Removed line uses theme.error (differs between themes)
        assert_eq!(dark_lines[1].spans[0].style.fg, Some(DARK_THEME.error));
        assert_eq!(light_lines[1].spans[0].style.fg, Some(LIGHT_THEME.error));
        assert_ne!(
            dark_lines[1].spans[0].style.fg,
            light_lines[1].spans[0].style.fg,
        );

        // Hunk header uses theme.diff_hunk (differs between themes)
        assert_eq!(dark_lines[2].spans[0].style.fg, Some(DARK_THEME.diff_hunk));
        assert_eq!(
            light_lines[2].spans[0].style.fg,
            Some(LIGHT_THEME.diff_hunk)
        );
        assert_ne!(
            dark_lines[2].spans[0].style.fg,
            light_lines[2].spans[0].style.fg,
        );
    }
}
