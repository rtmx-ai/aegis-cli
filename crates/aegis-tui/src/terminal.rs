//! SSH and tmux rendering compatibility detection.
//!
//! Detects constrained terminal environments (SSH, tmux, GNU screen) and
//! determines the color depth available, so the TUI can fall back to
//! 256-color or 16-color palettes when true color is not supported.

use std::env;

/// Supported color depths, from richest to most constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorDepth {
    /// 24-bit true color (16 million colors).
    TrueColor,
    /// 256-color xterm palette.
    Color256,
    /// Classic 16-color ANSI palette.
    Color16,
    /// No color support (e.g. dumb terminal).
    Monochrome,
}

/// Snapshot of the detected terminal environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEnv {
    /// Running inside an SSH session.
    pub is_ssh: bool,
    /// Running inside tmux.
    pub is_tmux: bool,
    /// Running inside GNU screen.
    pub is_screen: bool,
    /// Detected color depth.
    pub color_depth: ColorDepth,
}

/// Detect the current terminal environment from process environment variables.
///
/// Checks `TERM`, `COLORTERM`, `TMUX`, `SSH_CONNECTION`, `SSH_CLIENT`, and
/// `STY` to build a [`TerminalEnv`] snapshot.
pub fn detect_env() -> TerminalEnv {
    let term = env::var("TERM").unwrap_or_default();
    let colorterm = env::var("COLORTERM").ok();

    TerminalEnv {
        is_ssh: is_ssh_session(),
        is_tmux: is_tmux_session(),
        is_screen: is_screen_session(),
        color_depth: detect_color_depth(&term, colorterm.as_deref()),
    }
}

/// Detect the current terminal environment from explicit values.
///
/// This is the testable core -- callers supply the raw env var values.
pub fn detect_env_from(
    term: &str,
    colorterm: Option<&str>,
    ssh_connection: Option<&str>,
    ssh_client: Option<&str>,
    tmux: Option<&str>,
    sty: Option<&str>,
) -> TerminalEnv {
    TerminalEnv {
        is_ssh: ssh_connection.is_some() || ssh_client.is_some(),
        is_tmux: tmux.is_some(),
        is_screen: sty.is_some(),
        color_depth: detect_color_depth(term, colorterm),
    }
}

/// Determine color depth from `TERM` and `COLORTERM` values.
///
/// Rules (evaluated in order):
/// 1. `COLORTERM` = `truecolor` or `24bit` -> [`ColorDepth::TrueColor`]
/// 2. `TERM` contains `256color` -> [`ColorDepth::Color256`]
/// 3. `TERM` = `dumb` or empty -> [`ColorDepth::Monochrome`]
/// 4. Otherwise -> [`ColorDepth::Color16`]
pub fn detect_color_depth(term: &str, colorterm: Option<&str>) -> ColorDepth {
    // COLORTERM takes precedence when it explicitly signals true color.
    if let Some(ct) = colorterm {
        let ct_lower = ct.to_lowercase();
        if ct_lower == "truecolor" || ct_lower == "24bit" {
            return ColorDepth::TrueColor;
        }
    }

    // Inspect $TERM for 256-color or dumb terminals.
    let term_lower = term.to_lowercase();
    if term_lower.contains("256color") {
        return ColorDepth::Color256;
    }
    if term_lower.is_empty() || term_lower == "dumb" {
        return ColorDepth::Monochrome;
    }

    ColorDepth::Color16
}

/// Returns `true` when rendering should be simplified for the environment.
///
/// Simplification is recommended when:
/// - Running over SSH (latency, bandwidth)
/// - Running inside tmux or screen (may strip true color)
/// - Color depth is [`ColorDepth::Monochrome`]
pub fn should_simplify_rendering(env: &TerminalEnv) -> bool {
    env.is_ssh || env.is_tmux || env.is_screen || env.color_depth == ColorDepth::Monochrome
}

/// Determine whether the TUI should be bypassed in favor of plain-text mode.
///
/// Returns `true` when any of the following conditions hold:
/// - `no_tui_flag` is `true` (the `--no-tui` CLI flag was passed)
/// - The `NO_COLOR` environment variable is set (any value, per <https://no-color.org/>)
/// - `TERM` is `dumb` or empty
///
/// This is the testable core -- callers supply the flag value and raw env
/// var values so the function is deterministic and safe for parallel tests.
pub fn should_use_plain_text_from(no_tui_flag: bool, term: &str, no_color: Option<&str>) -> bool {
    if no_tui_flag {
        return true;
    }
    if no_color.is_some() {
        return true;
    }
    let t = term.to_lowercase();
    t.is_empty() || t == "dumb"
}

/// Convenience wrapper that reads the live process environment.
pub fn should_use_plain_text(no_tui_flag: bool) -> bool {
    let term = env::var("TERM").unwrap_or_default();
    let no_color = env::var("NO_COLOR").ok();
    should_use_plain_text_from(no_tui_flag, &term, no_color.as_deref())
}

// --- private helpers -------------------------------------------------------

fn is_ssh_session() -> bool {
    env::var("SSH_CONNECTION").is_ok() || env::var("SSH_CLIENT").is_ok()
}

fn is_tmux_session() -> bool {
    env::var("TMUX").is_ok()
}

fn is_screen_session() -> bool {
    env::var("STY").is_ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ColorDepth detection -----------------------------------------------

    #[test]
    // @req REQ-TUI-014
    fn truecolor_via_colorterm() {
        assert_eq!(
            detect_color_depth("xterm-256color", Some("truecolor")),
            ColorDepth::TrueColor,
        );
    }

    #[test]
    // @req REQ-TUI-014
    fn truecolor_via_24bit() {
        assert_eq!(
            detect_color_depth("xterm", Some("24bit")),
            ColorDepth::TrueColor,
        );
    }

    #[test]
    // @req REQ-TUI-014
    fn color256_via_term() {
        assert_eq!(
            detect_color_depth("xterm-256color", None),
            ColorDepth::Color256,
        );
    }

    #[test]
    // @req REQ-TUI-014
    fn color256_via_screen_term() {
        assert_eq!(
            detect_color_depth("screen-256color", None),
            ColorDepth::Color256,
        );
    }

    #[test]
    // @req REQ-TUI-014
    fn color16_plain_xterm() {
        assert_eq!(detect_color_depth("xterm", None), ColorDepth::Color16,);
    }

    #[test]
    // @req REQ-TUI-014
    fn monochrome_dumb_terminal() {
        assert_eq!(detect_color_depth("dumb", None), ColorDepth::Monochrome,);
    }

    #[test]
    // @req REQ-TUI-014
    fn monochrome_empty_term() {
        assert_eq!(detect_color_depth("", None), ColorDepth::Monochrome,);
    }

    #[test]
    // @req REQ-TUI-014
    fn colorterm_overrides_term() {
        // Even though TERM says 256color, COLORTERM=truecolor wins.
        assert_eq!(
            detect_color_depth("xterm-256color", Some("truecolor")),
            ColorDepth::TrueColor,
        );
    }

    // -- TerminalEnv detection via detect_env_from --------------------------

    #[test]
    // @req REQ-TUI-014
    fn detect_ssh_via_ssh_connection() {
        let env = detect_env_from(
            "xterm",
            None,
            Some("10.0.0.1 12345 10.0.0.2 22"),
            None,
            None,
            None,
        );
        assert!(env.is_ssh);
        assert!(!env.is_tmux);
        assert!(!env.is_screen);
    }

    #[test]
    // @req REQ-TUI-014
    fn detect_ssh_via_ssh_client() {
        let env = detect_env_from("xterm", None, None, Some("10.0.0.1 12345 22"), None, None);
        assert!(env.is_ssh);
    }

    #[test]
    // @req REQ-TUI-014
    fn detect_tmux_session() {
        let env = detect_env_from(
            "screen-256color",
            None,
            None,
            None,
            Some("/tmp/tmux-1000/default,1234,0"),
            None,
        );
        assert!(env.is_tmux);
        assert!(!env.is_ssh);
        assert!(!env.is_screen);
        assert_eq!(env.color_depth, ColorDepth::Color256);
    }

    #[test]
    // @req REQ-TUI-014
    fn detect_screen_session() {
        let env = detect_env_from(
            "screen",
            None,
            None,
            None,
            None,
            Some("1234.pts-0.hostname"),
        );
        assert!(env.is_screen);
        assert!(!env.is_tmux);
    }

    #[test]
    // @req REQ-TUI-014
    fn detect_plain_local_terminal() {
        let env = detect_env_from("xterm-256color", Some("truecolor"), None, None, None, None);
        assert!(!env.is_ssh);
        assert!(!env.is_tmux);
        assert!(!env.is_screen);
        assert_eq!(env.color_depth, ColorDepth::TrueColor);
    }

    // -- should_simplify_rendering ------------------------------------------

    #[test]
    // @req REQ-TUI-014
    fn simplify_for_ssh() {
        let env = TerminalEnv {
            is_ssh: true,
            is_tmux: false,
            is_screen: false,
            color_depth: ColorDepth::TrueColor,
        };
        assert!(should_simplify_rendering(&env));
    }

    #[test]
    // @req REQ-TUI-014
    fn simplify_for_tmux() {
        let env = TerminalEnv {
            is_ssh: false,
            is_tmux: true,
            is_screen: false,
            color_depth: ColorDepth::Color256,
        };
        assert!(should_simplify_rendering(&env));
    }

    #[test]
    // @req REQ-TUI-014
    fn simplify_for_screen() {
        let env = TerminalEnv {
            is_ssh: false,
            is_tmux: false,
            is_screen: true,
            color_depth: ColorDepth::Color16,
        };
        assert!(should_simplify_rendering(&env));
    }

    #[test]
    // @req REQ-TUI-014
    fn simplify_for_monochrome() {
        let env = TerminalEnv {
            is_ssh: false,
            is_tmux: false,
            is_screen: false,
            color_depth: ColorDepth::Monochrome,
        };
        assert!(should_simplify_rendering(&env));
    }

    #[test]
    // @req REQ-TUI-014
    fn no_simplify_for_local_truecolor() {
        let env = TerminalEnv {
            is_ssh: false,
            is_tmux: false,
            is_screen: false,
            color_depth: ColorDepth::TrueColor,
        };
        assert!(!should_simplify_rendering(&env));
    }

    #[test]
    // @req REQ-TUI-014
    fn no_simplify_for_local_256color() {
        let env = TerminalEnv {
            is_ssh: false,
            is_tmux: false,
            is_screen: false,
            color_depth: ColorDepth::Color256,
        };
        assert!(!should_simplify_rendering(&env));
    }

    // -- should_use_plain_text_from (REQ-TUI-013) -----------------------------

    #[test]
    // @req REQ-TUI-013
    fn plain_text_when_no_tui_flag() {
        assert!(should_use_plain_text_from(true, "xterm-256color", None));
    }

    #[test]
    // @req REQ-TUI-013
    fn plain_text_when_no_color_set() {
        assert!(should_use_plain_text_from(
            false,
            "xterm-256color",
            Some("")
        ));
    }

    #[test]
    // @req REQ-TUI-013
    fn plain_text_when_no_color_set_with_value() {
        assert!(should_use_plain_text_from(false, "xterm", Some("1")));
    }

    #[test]
    // @req REQ-TUI-013
    fn plain_text_when_term_dumb() {
        assert!(should_use_plain_text_from(false, "dumb", None));
    }

    #[test]
    // @req REQ-TUI-013
    fn plain_text_when_term_empty() {
        assert!(should_use_plain_text_from(false, "", None));
    }

    #[test]
    // @req REQ-TUI-013
    fn no_plain_text_normal_terminal() {
        assert!(!should_use_plain_text_from(false, "xterm-256color", None));
    }

    #[test]
    // @req REQ-TUI-013
    fn plain_text_flag_overrides_good_terminal() {
        // Even with a good terminal, the flag forces plain text.
        assert!(should_use_plain_text_from(true, "xterm-256color", None));
    }

    #[test]
    // @req REQ-TUI-013
    fn plain_text_term_dumb_case_insensitive() {
        assert!(should_use_plain_text_from(false, "DUMB", None));
    }
}
