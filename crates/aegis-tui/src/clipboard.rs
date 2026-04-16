//! System clipboard integration via arboard.
//!
//! Provides a single, uniform helper for writing text to the OS clipboard.
//! On headless environments (CI runners without a display server), the
//! clipboard is unavailable; this helper returns a user-friendly error
//! rather than panicking.

/// Copy text to the system clipboard. Returns `Err` with a user-friendly
/// message if the clipboard is unavailable (headless / no display).
pub fn copy_text(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
    cb.set_text(text.to_string())
        .map_err(|e| format!("clipboard write failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-021
    #[test]
    fn copy_text_returns_ok_or_unavailable_error() {
        // The clipboard may be unavailable in headless CI. Either Ok or a
        // well-formed error string is acceptable; we only verify we do not
        // panic and that errors carry an actionable prefix.
        match copy_text("hello") {
            Ok(()) => {}
            Err(msg) => {
                assert!(
                    msg.starts_with("clipboard unavailable")
                        || msg.starts_with("clipboard write failed"),
                    "unexpected error message: {msg}"
                );
            }
        }
    }
}
