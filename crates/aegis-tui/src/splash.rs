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

/// Number of ticks between trivia rotations during model loading (~4 s).
pub const TRIVIA_ROTATE_TICKS: u16 = 27;

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

/// Spinner frames for the loading animation.
const SPINNER: &[&str] = &["   ", ".  ", ".. ", "...", " ..", "  .", "   "];

/// Render the splash screen with model loading state and trivia carousel.
///
/// REQ-TUI-111: Shows the logo, a "Loading <model>..." indicator with spinner,
/// and a rotating defense AI trivia fact from REQ-TUI-112.
pub fn render_loading_splash(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    model_name: &str,
    ticks: u16,
) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    // Logo in accent color
    for logo_line in brand::LOGO_FULL.lines() {
        lines.push(Line::from(Span::styled(
            logo_line.to_string(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));

    // Loading indicator with spinner
    let spinner_frame = SPINNER[(ticks as usize / 2) % SPINNER.len()];
    lines.push(Line::from(vec![
        Span::styled(
            format!("Loading {model_name} for local AI"),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(spinner_frame.to_string(), Style::default().fg(theme.accent)),
    ]));

    lines.push(Line::from(""));

    // Trivia fact, rotating every TRIVIA_ROTATE_TICKS
    let trivia_index = (ticks / TRIVIA_ROTATE_TICKS) as usize;
    let fact = crate::trivia::fact_by_index(trivia_index);

    // Word-wrap the fact to fit the area width (with padding)
    let max_width = area.width.saturating_sub(4) as usize;
    for wrapped_line in wrap_text(fact, max_width) {
        lines.push(Line::from(Span::styled(
            wrapped_line,
            Style::default()
                .fg(theme.border)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    lines.push(Line::from(""));

    // Dismiss hint
    lines.push(Line::from(Span::styled(
        "Press any key to continue",
        Style::default().fg(theme.border),
    )));

    let total_height = lines.len() as u16;

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

/// Simple word wrap: split text into lines that fit within max_width.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
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

    fn render_loading_to_string(width: u16, height: u16, model: &str, ticks: u16) -> String {
        use crate::theme::DARK_THEME;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_loading_splash(frame, frame.area(), &DARK_THEME, model, ticks))
            .unwrap();
        terminal.backend().to_string()
    }

    // rtmx:req REQ-TUI-111
    #[test]
    fn loading_splash_shows_model_name() {
        let output = render_loading_to_string(80, 30, "gemma3:4b", 0);
        assert!(
            output.contains("gemma3:4b"),
            "Loading splash should show model name: {output}"
        );
    }

    // rtmx:req REQ-TUI-111
    #[test]
    fn loading_splash_shows_loading_text() {
        let output = render_loading_to_string(80, 30, "llama3", 0);
        assert!(
            output.contains("Loading"),
            "Loading splash should show 'Loading': {output}"
        );
        assert!(
            output.contains("local AI"),
            "Loading splash should show 'local AI': {output}"
        );
    }

    // rtmx:req REQ-TUI-111
    #[test]
    fn loading_splash_shows_logo() {
        let output = render_loading_to_string(80, 30, "llama3", 0);
        assert!(
            output.contains("###"),
            "Loading splash should show logo: {output}"
        );
    }

    // rtmx:req REQ-TUI-112
    #[test]
    fn loading_splash_shows_trivia() {
        let output = render_loading_to_string(100, 35, "llama3", 0);
        // First trivia fact mentions Dartmouth
        assert!(
            output.contains("Dartmouth") || output.contains("artificial intelligence"),
            "Loading splash should show trivia fact: {output}"
        );
    }

    // rtmx:req REQ-TUI-112
    #[test]
    fn loading_splash_trivia_rotates() {
        let output_a = render_loading_to_string(100, 35, "llama3", 0);
        let output_b = render_loading_to_string(100, 35, "llama3", TRIVIA_ROTATE_TICKS);
        // Different ticks should produce different trivia (different facts)
        assert_ne!(output_a, output_b, "Trivia should rotate between ticks");
    }

    // rtmx:req REQ-TUI-111
    #[test]
    fn loading_splash_does_not_panic_on_small_terminal() {
        let output = render_loading_to_string(40, 10, "llama3", 5);
        assert!(!output.is_empty());
    }

    #[test]
    fn wrap_text_basic() {
        let lines = wrap_text("hello world foo bar", 11);
        assert_eq!(lines, vec!["hello world", "foo bar"]);
    }

    #[test]
    fn wrap_text_single_long_word() {
        let lines = wrap_text("superlongword", 5);
        // Single word longer than max_width stays on one line
        assert_eq!(lines, vec!["superlongword"]);
    }

    #[test]
    fn wrap_text_empty() {
        let lines = wrap_text("", 80);
        assert_eq!(lines, vec![""]);
    }
}
