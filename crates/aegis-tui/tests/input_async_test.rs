//! Tests for non-blocking input during streaming (REQ-TUI-093)
//! and prompt queue with ordered processing (REQ-TUI-094).

use aegis_tui::app::{Action, AppPhase};
use aegis_tui::event::TuiEvent;
use crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

fn agent_tx() -> (
    mpsc::UnboundedSender<String>,
    mpsc::UnboundedReceiver<String>,
) {
    mpsc::unbounded_channel()
}

fn send_key(
    app: &mut aegis_tui::app::App,
    code: KeyCode,
    mods: KeyModifiers,
    tx: &mpsc::UnboundedSender<String>,
) -> Action {
    let event = TuiEvent::Terminal(CtEvent::Key(KeyEvent::new(code, mods)));
    app.handle_event(event, tx)
}

fn type_str(app: &mut aegis_tui::app::App, s: &str, tx: &mpsc::UnboundedSender<String>) {
    for ch in s.chars() {
        send_key(app, KeyCode::Char(ch), KeyModifiers::NONE, tx);
    }
}

fn press_enter(app: &mut aegis_tui::app::App, tx: &mpsc::UnboundedSender<String>) -> Action {
    send_key(app, KeyCode::Enter, KeyModifiers::NONE, tx)
}

// rtmx:req REQ-TUI-093
#[test]
fn test_input_accepts_keystrokes_during_streaming() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "hello", &tx);

    assert_eq!(
        app.input.text, "hello",
        "keystrokes must be captured during streaming"
    );
}

// rtmx:req REQ-TUI-093
#[test]
fn test_cursor_movement_works_during_streaming() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "abc", &tx);
    assert_eq!(app.input.cursor, 3);

    // Move left
    send_key(&mut app, KeyCode::Left, KeyModifiers::NONE, &tx);
    assert_eq!(
        app.input.cursor, 2,
        "cursor must move left during streaming"
    );

    // Backspace
    send_key(&mut app, KeyCode::Backspace, KeyModifiers::NONE, &tx);
    assert_eq!(app.input.text, "ac", "backspace must work during streaming");
}

// rtmx:req REQ-TUI-093
#[test]
fn test_ctrl_c_still_quits_during_streaming() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    // REQ-AGENT-064: First Ctrl+C triggers graceful cancel (KillSwitch),
    // not immediate quit. Second Ctrl+C within 2s forces Quit.
    let action = send_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL, &tx);
    assert_eq!(
        action,
        Action::KillSwitch,
        "First Ctrl+C during streaming triggers graceful cancel"
    );
}

// rtmx:req REQ-TUI-093
#[test]
fn test_input_works_during_tool_executing() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::ToolExecuting;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "next task", &tx);

    assert_eq!(
        app.input.text, "next task",
        "keystrokes must be captured during tool execution"
    );
}

// rtmx:req REQ-TUI-094
#[test]
fn test_enter_during_streaming_queues_prompt() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "queued prompt", &tx);
    press_enter(&mut app, &tx);

    assert_eq!(app.prompt_queue.len(), 1, "prompt must be queued");
    assert_eq!(app.prompt_queue[0], "queued prompt");
    assert!(
        app.input.text.is_empty(),
        "input must be cleared after submit"
    );
}

// rtmx:req REQ-TUI-094
#[test]
fn test_queued_prompt_appears_in_chat() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "follow-up", &tx);
    press_enter(&mut app, &tx);

    let user_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == aegis_tui::messages::MessageKind::User)
        .collect();
    assert_eq!(
        user_msgs.len(),
        1,
        "queued prompt must appear in chat immediately"
    );
    assert_eq!(user_msgs[0].content, "follow-up");
}

// rtmx:req REQ-TUI-094
#[test]
fn test_prompt_queue_fifo_processing() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, mut rx) = agent_tx();

    // Queue two prompts
    type_str(&mut app, "first", &tx);
    press_enter(&mut app, &tx);
    type_str(&mut app, "second", &tx);
    press_enter(&mut app, &tx);

    assert_eq!(app.prompt_queue.len(), 2);

    // Simulate AgentDone -- should drain first queued prompt
    let done = TuiEvent::AgentDone {
        input_tokens: 100,
        output_tokens: 50,
    };
    app.handle_event(done, &tx);

    // First queued prompt should have been sent to agent
    let sent = rx
        .try_recv()
        .expect("first queued prompt must be sent to agent");
    assert_eq!(sent, "first");
    assert_eq!(
        app.phase,
        AppPhase::Streaming,
        "phase must return to Streaming"
    );
    assert_eq!(app.prompt_queue.len(), 1, "one prompt must remain in queue");

    // Second AgentDone drains second prompt
    let done2 = TuiEvent::AgentDone {
        input_tokens: 80,
        output_tokens: 40,
    };
    app.handle_event(done2, &tx);
    let sent2 = rx
        .try_recv()
        .expect("second queued prompt must be sent to agent");
    assert_eq!(sent2, "second");
    assert_eq!(app.prompt_queue.len(), 0, "queue must be empty");

    // Third AgentDone with empty queue returns to Idle
    let done3 = TuiEvent::AgentDone {
        input_tokens: 60,
        output_tokens: 30,
    };
    app.handle_event(done3, &tx);
    assert_eq!(
        app.phase,
        AppPhase::Idle,
        "phase must be Idle when queue is empty"
    );
}

// rtmx:req REQ-TUI-094
#[test]
fn test_ctrl_c_clears_queue() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    // Queue a prompt
    type_str(&mut app, "will be cleared", &tx);
    press_enter(&mut app, &tx);
    assert_eq!(app.prompt_queue.len(), 1);

    // Ctrl+C should clear the queue
    send_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL, &tx);

    assert!(
        app.prompt_queue.is_empty(),
        "Ctrl+C must clear the prompt queue"
    );
}

// rtmx:req REQ-TUI-094
#[test]
fn test_queue_depth_in_status() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    assert_eq!(app.status_info().queue_depth, 0);

    type_str(&mut app, "a", &tx);
    press_enter(&mut app, &tx);
    assert_eq!(app.status_info().queue_depth, 1);

    type_str(&mut app, "b", &tx);
    press_enter(&mut app, &tx);
    assert_eq!(app.status_info().queue_depth, 2);
}

// rtmx:req REQ-TUI-094
#[test]
fn test_queue_depth_shown_in_status_text() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "queued", &tx);
    press_enter(&mut app, &tx);

    let status = app.status_text();
    assert!(
        status.contains("1 queued"),
        "status text must show queue depth: {status}"
    );
}

// rtmx:req REQ-TUI-093
#[test]
fn test_empty_enter_during_streaming_does_not_queue() {
    let mut app = aegis_tui::app::App::new("test-model");
    app.phase = AppPhase::Streaming;
    let (tx, _rx) = agent_tx();

    // Press Enter with empty input
    press_enter(&mut app, &tx);

    assert!(app.prompt_queue.is_empty(), "empty submit must not queue");
}
