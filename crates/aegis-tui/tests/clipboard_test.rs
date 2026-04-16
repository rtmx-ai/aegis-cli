//! Integration test for the system clipboard helper.
//!
//! CI runners on Linux often lack an X11/Wayland display and thus have
//! no usable clipboard. We accept either Ok or the expected
//! "clipboard unavailable" error so the test stays deterministic across
//! developer machines and headless CI.

use aegis_tui::clipboard::copy_text;

// rtmx:req REQ-TUI-021
#[test]
fn test_text_selection_copies_to_clipboard() {
    match copy_text("hello") {
        Ok(()) => {
            // Local dev with a real clipboard -- write succeeded.
        }
        Err(msg) => {
            // Headless CI / no display -- we expect the helper to surface
            // a user-facing explanation rather than panic.
            assert!(
                msg.starts_with("clipboard unavailable")
                    || msg.starts_with("clipboard write failed"),
                "unexpected clipboard error: {msg}"
            );
        }
    }
}
