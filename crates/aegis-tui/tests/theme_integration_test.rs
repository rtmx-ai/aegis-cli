//! Theme integration tests: verify theme plumbing, extended palette,
//! and visual polish requirements.

use aegis_tui::layout::AppState;
use aegis_tui::theme::{DARK_THEME, LIGHT_THEME};

// rtmx:req REQ-TUI-095
#[test]
fn test_extended_theme_has_all_17_slots() {
    // All 17 color slots must be non-Reset for both themes.
    use ratatui::style::Color;
    let d = Color::Reset;
    for theme in [&DARK_THEME, &LIGHT_THEME] {
        // Original 8 slots
        assert_ne!(theme.bg, d, "{} bg", theme.name);
        assert_ne!(theme.fg, d, "{} fg", theme.name);
        assert_ne!(theme.accent, d, "{} accent", theme.name);
        assert_ne!(theme.error, d, "{} error", theme.name);
        assert_ne!(theme.warning, d, "{} warning", theme.name);
        assert_ne!(theme.code_bg, d, "{} code_bg", theme.name);
        assert_ne!(theme.border, d, "{} border", theme.name);
        assert_ne!(theme.status_bg, d, "{} status_bg", theme.name);
        // 9 new semantic slots
        assert_ne!(theme.success, d, "{} success", theme.name);
        assert_ne!(theme.streaming, d, "{} streaming", theme.name);
        assert_ne!(theme.approval_pending, d, "{} approval_pending", theme.name);
        assert_ne!(theme.message_user, d, "{} message_user", theme.name);
        assert_ne!(
            theme.message_assistant, d,
            "{} message_assistant",
            theme.name
        );
        assert_ne!(theme.message_system, d, "{} message_system", theme.name);
        assert_ne!(theme.metric_secondary, d, "{} metric_secondary", theme.name);
        assert_ne!(theme.directory, d, "{} directory", theme.name);
        assert_ne!(theme.diff_hunk, d, "{} diff_hunk", theme.name);
    }
}

// rtmx:req REQ-TUI-095
#[test]
fn test_dark_and_light_differ_on_semantic_slots() {
    // Dark and light themes should have distinct values for key slots.
    assert_ne!(DARK_THEME.success, LIGHT_THEME.success);
    assert_ne!(DARK_THEME.message_user, LIGHT_THEME.message_user);
    assert_ne!(DARK_THEME.message_assistant, LIGHT_THEME.message_assistant);
}

// rtmx:req REQ-TUI-096
#[test]
fn test_appstate_contains_theme() {
    let state = AppState::default();
    assert_eq!(state.theme, DARK_THEME);
}

// rtmx:req REQ-TUI-096
#[test]
fn test_appstate_theme_overridable() {
    let state = AppState {
        theme: LIGHT_THEME,
        ..Default::default()
    };
    assert_eq!(state.theme, LIGHT_THEME);
}

// rtmx:req REQ-TUI-095
#[test]
fn test_downgrade_handles_all_17_slots() {
    use aegis_tui::theme::downgrade_theme;
    use ratatui::style::Color;
    let downgraded = downgrade_theme(&DARK_THEME);
    let all_colors = [
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
    for (i, color) in all_colors.iter().enumerate() {
        assert!(
            matches!(color, Color::Indexed(_)),
            "slot {i} should be Indexed after downgrade, got {:?}",
            color
        );
    }
}
