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
        // Verify all 8 named color slots are non-default (not Color::Reset).
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
