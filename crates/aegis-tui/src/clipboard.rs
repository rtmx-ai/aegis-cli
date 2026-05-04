//! System clipboard integration with OSC 52 passthrough for remote sessions.
//!
//! Provides clipboard helpers for writing text to the OS clipboard.
//! When running inside SSH or tmux sessions, uses OSC 52 escape sequences
//! to forward clipboard data through the terminal multiplexer. Falls back
//! to arboard when not in a remote session or when OSC 52 fails.

use std::io::Write;

/// Returns `true` when the process is running inside an SSH or tmux session.
///
/// Checks `SSH_CONNECTION`, `SSH_TTY`, and `TMUX` environment variables.
pub fn is_remote_session() -> bool {
    std::env::var("SSH_CONNECTION").is_ok()
        || std::env::var("SSH_TTY").is_ok()
        || std::env::var("TMUX").is_ok()
}

/// Encode bytes to base64 (RFC 4648, standard alphabet, with padding).
///
/// Implemented inline to avoid adding a `base64` crate dependency.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Build the OSC 52 escape sequence string for the given text.
///
/// The sequence is: `ESC ] 52 ; c ; <base64> BEL`
pub fn build_osc52_sequence(text: &str) -> String {
    let encoded = base64_encode(text.as_bytes());
    format!("\x1b]52;c;{encoded}\x07")
}

/// Write the OSC 52 clipboard escape sequence to stdout.
///
/// This sends the text to the terminal, which (if the terminal supports
/// OSC 52) will copy it to the system clipboard. Works through SSH and
/// tmux sessions.
pub fn osc52_copy(text: &str) -> Result<(), String> {
    let seq = build_osc52_sequence(text);
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(seq.as_bytes())
        .map_err(|e| format!("OSC 52 write failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("OSC 52 flush failed: {e}"))
}

/// Copy text to the system clipboard.
///
/// When running in a remote session (SSH or tmux), uses OSC 52 escape
/// sequences. Otherwise falls back to arboard. If the primary method
/// fails in a remote session, also attempts the arboard fallback.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    if is_remote_session() {
        match osc52_copy(text) {
            Ok(()) => return Ok(()),
            Err(osc_err) => {
                // Fallback to arboard
                return copy_text_arboard(text).map_err(|arboard_err| {
                    format!("OSC 52 failed ({osc_err}), arboard also failed ({arboard_err})")
                });
            }
        }
    }
    copy_text_arboard(text)
}

/// Copy text to the system clipboard via arboard.
///
/// Returns `Err` with a user-friendly message if the clipboard is
/// unavailable (headless / no display).
pub fn copy_text(text: &str) -> Result<(), String> {
    copy_to_clipboard(text)
}

/// Arboard-only clipboard copy (internal helper).
fn copy_text_arboard(text: &str) -> Result<(), String> {
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
                        || msg.starts_with("clipboard write failed")
                        || msg.contains("arboard also failed"),
                    "unexpected error message: {msg}"
                );
            }
        }
    }

    // rtmx:req REQ-TUI-023
    #[test]
    fn test_osc52_clipboard_passthrough() {
        let seq = build_osc52_sequence("hello");
        // "hello" in base64 is "aGVsbG8="
        assert_eq!(seq, "\x1b]52;c;aGVsbG8=\x07");
    }

    // rtmx:req REQ-TUI-023
    #[test]
    fn test_osc52_clipboard_passthrough_empty() {
        let seq = build_osc52_sequence("");
        // empty string in base64 is ""
        assert_eq!(seq, "\x1b]52;c;\x07");
    }

    // rtmx:req REQ-TUI-023
    #[test]
    fn test_is_remote_session_detects_ssh() {
        // We cannot safely mutate real env vars in parallel tests, so
        // we verify the function reads the expected variables by checking
        // the current environment. In CI (no SSH), this should be false.
        // The logic is tested structurally: the function checks three
        // specific env vars.
        let has_ssh = std::env::var("SSH_CONNECTION").is_ok()
            || std::env::var("SSH_TTY").is_ok()
            || std::env::var("TMUX").is_ok();
        assert_eq!(is_remote_session(), has_ssh);
    }

    // rtmx:req REQ-TUI-023
    #[test]
    fn test_is_remote_session_false_locally() {
        // When none of SSH_CONNECTION, SSH_TTY, or TMUX are set, the
        // function should return false. We verify the logic is consistent
        // with the env state.
        if std::env::var("SSH_CONNECTION").is_err()
            && std::env::var("SSH_TTY").is_err()
            && std::env::var("TMUX").is_err()
        {
            assert!(!is_remote_session());
        }
    }

    // rtmx:req REQ-TUI-023
    #[test]
    fn test_copy_falls_back_to_arboard() {
        // When not in a remote session, copy_to_clipboard should use the
        // arboard path. We verify it does not panic and returns a result.
        if !is_remote_session() {
            let result = copy_to_clipboard("test fallback");
            // In CI without display, arboard may fail -- that is fine.
            match result {
                Ok(()) => {}
                Err(msg) => {
                    assert!(
                        msg.contains("clipboard unavailable")
                            || msg.contains("clipboard write failed"),
                        "unexpected error: {msg}"
                    );
                }
            }
        }
    }

    // rtmx:req REQ-TUI-023
    #[test]
    fn test_base64_encode_correctness() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
    }
}
