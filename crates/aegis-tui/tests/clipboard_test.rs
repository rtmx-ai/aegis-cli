//! Integration test for the system clipboard helper.
//!
//! CI runners on Linux often lack an X11/Wayland display and thus have
//! no usable clipboard. We accept either Ok or the expected
//! "clipboard unavailable" error so the test stays deterministic across
//! developer machines and headless CI.

use aegis_tui::clipboard::{copy_text, copy_to_clipboard};

// rtmx:req REQ-TUI-009
#[test]
fn test_clipboard_copy_paste() {
    // REQ-TUI-009 parent: clipboard integration via arboard + OSC 52 fallback.
    // Both copy_text (arboard primary) and copy_to_clipboard (with OSC 52
    // fallback for SSH/remote sessions) must work or report a user-facing
    // error on headless CI.
    for (label, result) in [
        ("copy_text", copy_text("REQ-TUI-009 clipboard test")),
        (
            "copy_to_clipboard",
            copy_to_clipboard("REQ-TUI-009 clipboard test"),
        ),
    ] {
        match result {
            Ok(()) => {} // clipboard available
            Err(msg) => {
                assert!(
                    msg.starts_with("clipboard unavailable")
                        || msg.starts_with("clipboard write failed")
                        || msg.contains("arboard also failed"),
                    "{label}: unexpected clipboard error: {msg}"
                );
            }
        }
    }
}

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
