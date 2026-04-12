//! TUI snapshot tests using ratatui TestBackend + insta.
//!
//! These tests render the TUI in various states and capture snapshots
//! to detect visual regressions. Each test drives the TUI to a specific
//! state, renders to a TestBackend, and asserts the output matches the
//! stored snapshot.
//!
//! @req REQ-TEST-003

mod tui_harness;

use aegis_domain::types::FilePath;
use aegis_tui::app::AppPhase;
use aegis_tui::messages::ChatMessage;
use tui_harness::TuiHarness;

/// Standard terminal dimensions for snapshot tests.
const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_idle_fresh_app() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "gemini-2.0-flash");
    let frame = harness.render();
    insta::assert_snapshot!("idle_fresh_app", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_with_user_message() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "gemini-2.0-flash");
    harness.app_mut().messages.push(ChatMessage::user(
        "Explain the architecture of this codebase",
    ));
    let frame = harness.render();
    insta::assert_snapshot!("with_user_message", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_with_assistant_response() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "gemini-2.0-flash");
    harness
        .app_mut()
        .messages
        .push(ChatMessage::user("What does main.rs do?"));
    harness.app_mut().messages.push(ChatMessage::assistant(
        "The main.rs file initializes the application, sets up the TUI, and starts the event loop.",
    ));
    let frame = harness.render();
    insta::assert_snapshot!("with_assistant_response", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_with_streaming_buffer() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "gemini-2.0-flash");
    harness
        .app_mut()
        .messages
        .push(ChatMessage::user("Summarize this file"));
    // Simulate streaming: tokens accumulate in stream_buffer, phase is Streaming
    harness.send_token("The file contains ");
    harness.send_token("several important ");
    harness.send_token("functions that...");
    assert_eq!(harness.app().phase, AppPhase::Streaming);
    let frame = harness.render();
    insta::assert_snapshot!("with_streaming_buffer", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_status_line_idle() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "llama3-il5");
    let frame = harness.render();
    insta::assert_snapshot!("status_line_idle", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_status_line_streaming() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "llama3-il5");
    harness.send_token("hello");
    assert_eq!(harness.app().phase, AppPhase::Streaming);
    let frame = harness.render();
    insta::assert_snapshot!("status_line_streaming", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_status_line_tool_executing() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "llama3-il5");
    harness.send_tool_use(aegis_domain::types::ToolCall::ReadFile {
        path: FilePath::new_unchecked("src/main.rs"),
    });
    assert_eq!(harness.app().phase, AppPhase::ToolExecuting);
    let frame = harness.render();
    insta::assert_snapshot!("status_line_tool_executing", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_status_line_with_token_counts() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "llama3-il5");
    harness.send_token("done");
    harness.send_done(1500, 3200);
    assert_eq!(harness.app().phase, AppPhase::Idle);
    let frame = harness.render();
    insta::assert_snapshot!("status_line_with_token_counts", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_multi_turn_conversation() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "gemini-2.0-flash");
    harness
        .app_mut()
        .messages
        .push(ChatMessage::user("Read Cargo.toml"));
    harness
        .app_mut()
        .messages
        .push(ChatMessage::tool_call("read_file", "Cargo.toml (2.1KB)"));
    harness.app_mut().messages.push(ChatMessage::assistant(
        "The workspace has 10 crates under the crates/ directory.",
    ));
    harness
        .app_mut()
        .messages
        .push(ChatMessage::user("Which crate handles the TUI?"));
    harness.app_mut().messages.push(ChatMessage::assistant(
        "The aegis-tui crate handles the terminal user interface.",
    ));
    let frame = harness.render();
    insta::assert_snapshot!("multi_turn_conversation", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_error_message() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "gemini-2.0-flash");
    harness
        .app_mut()
        .messages
        .push(ChatMessage::user("Run the tests"));
    harness.app_mut().messages.push(ChatMessage::error(
        "Connection timed out: provider unreachable",
    ));
    let frame = harness.render();
    insta::assert_snapshot!("error_message", frame);
}

// rtmx:req REQ-TEST-003
#[test]
fn snapshot_system_message() {
    let mut harness = TuiHarness::new(WIDTH, HEIGHT, "gemini-2.0-flash");
    harness
        .app_mut()
        .messages
        .push(ChatMessage::system("Commands: /clear /help /context /quit"));
    let frame = harness.render();
    insta::assert_snapshot!("system_message", frame);
}
