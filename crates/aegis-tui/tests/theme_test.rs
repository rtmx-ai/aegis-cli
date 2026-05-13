//! Integration tests for the TUI theme system.
//!
//! Verifies that both dark and light themes render with legible contrast,
//! and that 256-color fallback produces valid indexed colors for all slots.

use aegis_tui::theme::{
    ColorSupport, DARK_THEME, LIGHT_THEME, detect_color_support_from, downgrade_theme,
    handle_theme_command,
};
use ratatui::style::Color;

// rtmx:req REQ-TUI-012
#[test]
fn test_theme_dark_light_256() {
    // Both themes exist and have distinct palettes
    assert_ne!(
        DARK_THEME.bg, LIGHT_THEME.bg,
        "themes must have distinct backgrounds"
    );
    assert_ne!(
        DARK_THEME.fg, LIGHT_THEME.fg,
        "themes must have distinct foregrounds"
    );

    // Dark theme: light text on dark background
    assert!(
        matches!(DARK_THEME.bg, Color::Rgb(r, _, _) if r < 100),
        "dark bg must be dark"
    );
    assert!(
        matches!(DARK_THEME.fg, Color::Rgb(r, _, _) if r > 150),
        "dark fg must be light"
    );

    // Light theme: dark text on light background
    assert!(
        matches!(LIGHT_THEME.bg, Color::Rgb(r, _, _) if r > 200),
        "light bg must be light"
    );
    assert!(
        matches!(LIGHT_THEME.fg, Color::Rgb(r, _, _) if r < 120),
        "light fg must be dark"
    );

    // 256-color fallback: all slots downgrade to indexed colors
    for theme in [&DARK_THEME, &LIGHT_THEME] {
        let downgraded = downgrade_theme(theme);
        for color in [
            downgraded.bg,
            downgraded.fg,
            downgraded.accent,
            downgraded.error,
            downgraded.warning,
            downgraded.code_bg,
            downgraded.border,
            downgraded.status_bg,
        ] {
            assert!(
                matches!(color, Color::Indexed(_)),
                "{}: all colors must downgrade to 256 indexed, got {:?}",
                theme.name,
                color
            );
        }
    }

    // Color detection works for all support levels
    assert_eq!(
        detect_color_support_from(Some("truecolor"), None),
        ColorSupport::TrueColor
    );
    assert_eq!(
        detect_color_support_from(Some("256color"), None),
        ColorSupport::Color256
    );
    assert_eq!(
        detect_color_support_from(None, Some("dumb")),
        ColorSupport::Basic
    );

    // /theme command works for both themes
    assert!(
        handle_theme_command("dark").is_some(),
        "/theme dark must work"
    );
    assert!(
        handle_theme_command("light").is_some(),
        "/theme light must work"
    );
    assert!(
        handle_theme_command("invalid").is_none(),
        "invalid theme must return None"
    );
}
