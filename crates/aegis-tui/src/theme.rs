//! Theme system for the aegis TUI.
//!
//! Provides named color slots, built-in dark/light themes, 256-color fallback
//! detection, and a `/theme` slash command handler.

use ratatui::style::Color;

/// Color support levels detected from the terminal environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSupport {
    TrueColor,
    Color256,
    Basic,
}

/// Named color slots for the TUI theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub error: Color,
    pub warning: Color,
    pub code_bg: Color,
    pub border: Color,
    pub status_bg: Color,
    // -- Semantic slots (REQ-TUI-095) --
    /// Positive indicators: auth healthy, cost display, diff added, input prompt.
    pub success: Color,
    /// Streaming phase indicator.
    pub streaming: Color,
    /// Awaiting approval phase indicator.
    pub approval_pending: Color,
    /// User message borders and prefix.
    pub message_user: Color,
    /// Assistant message borders.
    pub message_assistant: Color,
    /// System message text.
    pub message_system: Color,
    /// Secondary metric display (output tokens).
    pub metric_secondary: Color,
    /// Directory entries in file picker.
    pub directory: Color,
    /// Diff hunk header lines.
    pub diff_hunk: Color,
}

pub const DARK_THEME: Theme = Theme {
    name: "dark",
    bg: Color::Rgb(30, 30, 46),
    fg: Color::Rgb(205, 214, 244),
    accent: Color::Rgb(137, 180, 250),
    error: Color::Rgb(243, 139, 168),
    warning: Color::Rgb(249, 226, 175),
    code_bg: Color::Rgb(49, 50, 68),
    border: Color::Rgb(88, 91, 112),
    status_bg: Color::Rgb(49, 50, 68),
    success: Color::Rgb(166, 227, 161),
    streaming: Color::Rgb(137, 180, 250),
    approval_pending: Color::Rgb(249, 226, 175),
    message_user: Color::Rgb(148, 226, 213),
    message_assistant: Color::Rgb(137, 180, 250),
    message_system: Color::Rgb(116, 199, 236),
    metric_secondary: Color::Rgb(180, 190, 254),
    directory: Color::Rgb(137, 180, 250),
    diff_hunk: Color::Rgb(116, 199, 236),
};

pub const LIGHT_THEME: Theme = Theme {
    name: "light",
    bg: Color::Rgb(239, 241, 245),
    fg: Color::Rgb(76, 79, 105),
    accent: Color::Rgb(30, 102, 245),
    error: Color::Rgb(210, 15, 57),
    warning: Color::Rgb(223, 142, 29),
    code_bg: Color::Rgb(204, 208, 218),
    border: Color::Rgb(156, 160, 176),
    status_bg: Color::Rgb(204, 208, 218),
    success: Color::Rgb(64, 160, 43),
    streaming: Color::Rgb(30, 102, 245),
    approval_pending: Color::Rgb(223, 142, 29),
    message_user: Color::Rgb(23, 146, 153),
    message_assistant: Color::Rgb(30, 102, 245),
    message_system: Color::Rgb(32, 159, 181),
    metric_secondary: Color::Rgb(114, 135, 253),
    directory: Color::Rgb(30, 102, 245),
    diff_hunk: Color::Rgb(32, 159, 181),
};

/// Map an RGB color to the nearest xterm-256 color index (6x6x6 cube, indices 16-231).
fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let r_idx = ((r as u16 * 5 + 127) / 255) as u8;
    let g_idx = ((g as u16 * 5 + 127) / 255) as u8;
    let b_idx = ((b as u16 * 5 + 127) / 255) as u8;
    16 + 36 * r_idx + 6 * g_idx + b_idx
}

/// Downgrade a single color from RGB to its nearest 256-color indexed equivalent.
/// Non-RGB colors are returned unchanged.
fn downgrade_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Indexed(rgb_to_256(r, g, b)),
        other => other,
    }
}

/// Detect color support from real environment variables.
pub fn detect_color_support() -> ColorSupport {
    let colorterm = std::env::var("COLORTERM").ok();
    let term = std::env::var("TERM").ok();
    detect_color_support_from(colorterm.as_deref(), term.as_deref())
}

/// Detect color support from the given environment variable values.
///
/// This is the testable core of [`detect_color_support`].
pub fn detect_color_support_from(colorterm: Option<&str>, term: Option<&str>) -> ColorSupport {
    if let Some(ct) = colorterm {
        let ct_lower = ct.to_lowercase();
        if ct_lower == "truecolor" || ct_lower == "24bit" {
            return ColorSupport::TrueColor;
        }
        return ColorSupport::Color256;
    }
    match term {
        Some(t) if t != "dumb" => ColorSupport::Color256,
        _ => ColorSupport::Basic,
    }
}

/// Create a new theme with all RGB colors downgraded to their nearest 256-color
/// indexed equivalents.
pub fn downgrade_theme(theme: &Theme) -> Theme {
    Theme {
        name: theme.name,
        bg: downgrade_color(theme.bg),
        fg: downgrade_color(theme.fg),
        accent: downgrade_color(theme.accent),
        error: downgrade_color(theme.error),
        warning: downgrade_color(theme.warning),
        code_bg: downgrade_color(theme.code_bg),
        border: downgrade_color(theme.border),
        status_bg: downgrade_color(theme.status_bg),
        success: downgrade_color(theme.success),
        streaming: downgrade_color(theme.streaming),
        approval_pending: downgrade_color(theme.approval_pending),
        message_user: downgrade_color(theme.message_user),
        message_assistant: downgrade_color(theme.message_assistant),
        message_system: downgrade_color(theme.message_system),
        metric_secondary: downgrade_color(theme.metric_secondary),
        directory: downgrade_color(theme.directory),
        diff_hunk: downgrade_color(theme.diff_hunk),
    }
}

/// Handle the `/theme` slash command. Returns the requested built-in theme,
/// or `None` if the argument does not match a known theme name.
pub fn handle_theme_command(arg: &str) -> Option<Theme> {
    match arg.trim().to_lowercase().as_str() {
        "dark" => Some(DARK_THEME.clone()),
        "light" => Some(LIGHT_THEME.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-077
    #[test]
    fn test_dark_and_light_themes_compile() {
        // Both themes should have distinct bg and fg colors.
        assert_ne!(DARK_THEME.bg, DARK_THEME.fg);
        assert_ne!(LIGHT_THEME.bg, LIGHT_THEME.fg);
        assert_ne!(DARK_THEME.bg, LIGHT_THEME.bg);
        assert_ne!(DARK_THEME.fg, LIGHT_THEME.fg);
    }

    // rtmx:req REQ-TUI-077
    #[test]
    fn test_theme_has_all_slots() {
        // Verify all 17 named color slots are non-default (not Color::Reset).
        let default_color = Color::Reset;
        for theme in [&DARK_THEME, &LIGHT_THEME] {
            assert_ne!(theme.bg, default_color, "{} bg", theme.name);
            assert_ne!(theme.fg, default_color, "{} fg", theme.name);
            assert_ne!(theme.accent, default_color, "{} accent", theme.name);
            assert_ne!(theme.error, default_color, "{} error", theme.name);
            assert_ne!(theme.warning, default_color, "{} warning", theme.name);
            assert_ne!(theme.code_bg, default_color, "{} code_bg", theme.name);
            assert_ne!(theme.border, default_color, "{} border", theme.name);
            assert_ne!(theme.status_bg, default_color, "{} status_bg", theme.name);
            assert_ne!(theme.success, default_color, "{} success", theme.name);
            assert_ne!(theme.streaming, default_color, "{} streaming", theme.name);
            assert_ne!(
                theme.approval_pending, default_color,
                "{} approval_pending",
                theme.name
            );
            assert_ne!(
                theme.message_user, default_color,
                "{} message_user",
                theme.name
            );
            assert_ne!(
                theme.message_assistant, default_color,
                "{} message_assistant",
                theme.name
            );
            assert_ne!(
                theme.message_system, default_color,
                "{} message_system",
                theme.name
            );
            assert_ne!(
                theme.metric_secondary, default_color,
                "{} metric_secondary",
                theme.name
            );
            assert_ne!(theme.directory, default_color, "{} directory", theme.name);
            assert_ne!(theme.diff_hunk, default_color, "{} diff_hunk", theme.name);
        }
    }

    // rtmx:req REQ-TUI-095
    #[test]
    fn test_extended_theme_has_all_17_slots() {
        // Verify the 9 new semantic slots exist and are distinct from Reset.
        let d = Color::Reset;
        for theme in [&DARK_THEME, &LIGHT_THEME] {
            let slots = [
                ("success", theme.success),
                ("streaming", theme.streaming),
                ("approval_pending", theme.approval_pending),
                ("message_user", theme.message_user),
                ("message_assistant", theme.message_assistant),
                ("message_system", theme.message_system),
                ("metric_secondary", theme.metric_secondary),
                ("directory", theme.directory),
                ("diff_hunk", theme.diff_hunk),
            ];
            for (name, color) in &slots {
                assert_ne!(*color, d, "{} {} must not be Reset", theme.name, name);
            }
        }
    }

    // rtmx:req REQ-TUI-078
    #[test]
    fn test_detect_truecolor() {
        assert_eq!(
            detect_color_support_from(Some("truecolor"), None),
            ColorSupport::TrueColor
        );
        assert_eq!(
            detect_color_support_from(Some("24bit"), None),
            ColorSupport::TrueColor
        );
    }

    // rtmx:req REQ-TUI-078
    #[test]
    fn test_detect_256_color() {
        assert_eq!(
            detect_color_support_from(Some("256color"), None),
            ColorSupport::Color256
        );
        assert_eq!(
            detect_color_support_from(Some("other"), Some("xterm-256color")),
            ColorSupport::Color256
        );
    }

    // rtmx:req REQ-TUI-078
    #[test]
    fn test_detect_basic_color() {
        assert_eq!(
            detect_color_support_from(None, Some("dumb")),
            ColorSupport::Basic
        );
        assert_eq!(detect_color_support_from(None, None), ColorSupport::Basic);
    }

    // rtmx:req REQ-TUI-078
    #[test]
    fn test_256_color_fallback() {
        let downgraded = downgrade_theme(&DARK_THEME);
        let colors = [
            downgraded.bg,
            downgraded.fg,
            downgraded.accent,
            downgraded.error,
            downgraded.warning,
            downgraded.code_bg,
            downgraded.border,
            downgraded.status_bg,
            downgraded.success,
            downgraded.streaming,
            downgraded.approval_pending,
            downgraded.message_user,
            downgraded.message_assistant,
            downgraded.message_system,
            downgraded.metric_secondary,
            downgraded.directory,
            downgraded.diff_hunk,
        ];
        for color in &colors {
            assert!(
                matches!(color, Color::Indexed(_)),
                "expected Color::Indexed, got {:?}",
                color
            );
        }
    }

    // rtmx:req REQ-TUI-079
    #[test]
    fn test_handle_theme_command_dark() {
        let theme = handle_theme_command("dark").expect("should return dark theme");
        assert_eq!(theme, DARK_THEME);
    }

    // rtmx:req REQ-TUI-079
    #[test]
    fn test_handle_theme_command_light() {
        let theme = handle_theme_command("light").expect("should return light theme");
        assert_eq!(theme, LIGHT_THEME);
    }

    // rtmx:req REQ-TUI-079
    #[test]
    fn test_handle_theme_command_invalid() {
        assert!(handle_theme_command("monokai").is_none());
        assert!(handle_theme_command("").is_none());
    }
}
