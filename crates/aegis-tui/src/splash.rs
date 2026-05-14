//! Splash screen rendered on first launch.
//!
//! Displays the full ASCII shield logo centered on screen, with the version
//! string and brand promise below. Dismissed by any keypress or after a
//! configurable timeout (default 1.5 s).

use crate::brand;
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Number of 150 ms ticks before auto-dismiss (1.5 s / 0.15 s = 10 ticks).
pub const SPLASH_TIMEOUT_TICKS: u16 = 10;

/// Render the splash screen centered in the given area.
pub fn render_splash(frame: &mut Frame, area: Rect, theme: &Theme) {
    // Build the logo + text block
    let mut lines: Vec<Line> = Vec::new();

    // Blank line for top padding
    lines.push(Line::from(""));

    // Logo lines in cyan
    for logo_line in brand::LOGO_FULL.lines() {
        lines.push(Line::from(Span::styled(
            logo_line.to_string(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
    }

    // Blank separator
    lines.push(Line::from(""));

    // Version
    lines.push(Line::from(Span::styled(
        format!("v{}", brand::VERSION),
        Style::default().fg(theme.fg),
    )));

    // Brand promise
    lines.push(Line::from(Span::styled(
        brand::BRAND_PROMISE.to_string(),
        Style::default()
            .fg(theme.border)
            .add_modifier(Modifier::ITALIC),
    )));

    // Blank separator
    lines.push(Line::from(""));

    // Dismiss hint
    lines.push(Line::from(Span::styled(
        "Press any key to continue",
        Style::default().fg(theme.border),
    )));

    let total_height = lines.len() as u16;

    // Center vertically
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total_height),
            Constraint::Min(0),
        ])
        .split(area);

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, vertical[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_splash_to_string(width: u16, height: u16) -> String {
        use crate::theme::DARK_THEME;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_splash(frame, frame.area(), &DARK_THEME))
            .unwrap();
        terminal.backend().to_string()
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_renders_logo() {
        let output = render_splash_to_string(80, 24);
        // The logo contains the shield's chevron ">" character
        assert!(
            output.contains(">"),
            "Splash should contain logo chevron: {output}"
        );
        // And the hash-heavy border
        assert!(
            output.contains("###"),
            "Splash should contain shield border: {output}"
        );
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_renders_version() {
        let output = render_splash_to_string(80, 24);
        assert!(
            output.contains(&format!("v{}", brand::VERSION)),
            "Splash should show version: {output}"
        );
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_renders_brand_promise() {
        let output = render_splash_to_string(80, 24);
        // Check for a distinctive substring of the brand promise
        assert!(
            output.contains("CUI"),
            "Splash should contain brand promise: {output}"
        );
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_renders_dismiss_hint() {
        let output = render_splash_to_string(80, 24);
        assert!(
            output.contains("Press any key"),
            "Splash should show dismiss hint: {output}"
        );
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_does_not_panic_on_small_terminal() {
        // Should not panic even at minimal size
        let output = render_splash_to_string(40, 10);
        assert!(!output.is_empty());
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_timeout_ticks_is_reasonable() {
        // 10 ticks at 150ms = 1.5s
        assert_eq!(SPLASH_TIMEOUT_TICKS, 10);
    }

    // rtmx:req REQ-TUI-100
    #[test]
    fn test_splash_uses_theme_accent() {
        use crate::theme::{DARK_THEME, LIGHT_THEME};
        // Both themes should render without panic, confirming theme
        // parameterization works for both built-in themes.
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_splash(frame, frame.area(), &DARK_THEME))
            .unwrap();
        let dark_output = terminal.backend().to_string();
        assert!(
            dark_output.contains("###"),
            "dark theme splash should render logo: {dark_output}"
        );

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_splash(frame, frame.area(), &LIGHT_THEME))
            .unwrap();
        let light_output = terminal.backend().to_string();
        assert!(
            light_output.contains("###"),
            "light theme splash should render logo: {light_output}"
        );
    }
}
