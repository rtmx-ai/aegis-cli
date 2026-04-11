//! Immutable platform detection, captured once at startup.
//!
//! Drives OS-aware keybinding selection and hint text. Also probes terminal
//! capability for the Kitty keyboard protocol, which enables Shift+Enter
//! disambiguation.

/// Detected operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOS,
    Linux,
    Windows,
}

/// Immutable platform snapshot captured at startup.
#[derive(Debug, Clone)]
pub struct Platform {
    pub os: Os,
    /// Whether the terminal supports the Kitty keyboard protocol
    /// (enables Shift+Enter as distinct from Enter).
    pub enhanced_keyboard: bool,
    /// Human-readable label for the newline keybinding.
    pub newline_hint: &'static str,
    /// Human-readable label for the paste keybinding on this OS.
    pub paste_hint: &'static str,
}

impl Platform {
    /// Detect the current platform. Call once at startup.
    ///
    /// Probes the terminal for Kitty keyboard protocol support. When
    /// available, Shift+Enter works as a newline key. When not, the hint
    /// directs users to vim normal-mode `o`.
    pub fn detect() -> Self {
        let os = if cfg!(target_os = "macos") {
            Os::MacOS
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        };

        let enhanced_keyboard =
            crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);

        let newline_hint = if enhanced_keyboard {
            "Shift+Enter newline"
        } else {
            "Esc, o new line"
        };

        let paste_hint = match os {
            Os::MacOS => "Cmd+V paste",
            Os::Linux | Os::Windows => "Ctrl+Shift+V paste",
        };

        Self {
            os,
            enhanced_keyboard,
            newline_hint,
            paste_hint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_valid_platform() {
        let p = Platform::detect();
        #[cfg(target_os = "macos")]
        assert_eq!(p.os, Os::MacOS);
        #[cfg(target_os = "linux")]
        assert_eq!(p.os, Os::Linux);
        #[cfg(target_os = "windows")]
        assert_eq!(p.os, Os::Windows);
    }

    #[test]
    fn platform_is_immutable_snapshot() {
        let p1 = Platform::detect();
        let p2 = Platform::detect();
        assert_eq!(p1.os, p2.os);
        assert_eq!(p1.enhanced_keyboard, p2.enhanced_keyboard);
    }

    #[test]
    fn newline_hint_is_nonempty() {
        let p = Platform::detect();
        assert!(!p.newline_hint.is_empty());
    }

    #[test]
    fn paste_hint_is_nonempty() {
        let p = Platform::detect();
        assert!(!p.paste_hint.is_empty());
    }

    #[test]
    fn newline_hint_adapts_to_capability() {
        let p = Platform::detect();
        if p.enhanced_keyboard {
            assert!(p.newline_hint.contains("Shift+Enter"), "{}", p.newline_hint);
        } else {
            assert!(p.newline_hint.contains("Esc"), "{}", p.newline_hint);
        }
    }
}
