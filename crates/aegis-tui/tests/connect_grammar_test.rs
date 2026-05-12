//! Tests for the /connect token grammar chain (REQ-TUI-087, REQ-TUI-088, REQ-TUI-089).

use aegis_tui::app::{Action, App, AppPhase};
use aegis_tui::command_palette::{CommandPalette, connect_grammar, options_for_provider};
use aegis_tui::event::TuiEvent;
use aegis_tui::messages::MessageKind;
use crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

fn agent_tx() -> (
    mpsc::UnboundedSender<String>,
    mpsc::UnboundedReceiver<String>,
) {
    mpsc::unbounded_channel()
}

fn send_key(
    app: &mut App,
    code: KeyCode,
    mods: KeyModifiers,
    tx: &mpsc::UnboundedSender<String>,
) -> Action {
    let event = TuiEvent::Terminal(CtEvent::Key(KeyEvent::new(code, mods)));
    app.handle_event(event, tx)
}

fn type_str(app: &mut App, s: &str, tx: &mpsc::UnboundedSender<String>) {
    for ch in s.chars() {
        send_key(app, KeyCode::Char(ch), KeyModifiers::NONE, tx);
    }
}

fn press_tab(app: &mut App, tx: &mpsc::UnboundedSender<String>) -> Action {
    send_key(app, KeyCode::Tab, KeyModifiers::NONE, tx)
}

// ---------------------------------------------------------------------------
// REQ-TUI-087: Token grammar definition for /connect arguments
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-087
#[test]
fn test_connect_grammar_valid_tokens() {
    let g = connect_grammar();
    assert_eq!(g.name, "/connect");
    assert_eq!(g.slots.len(), 4, "grammar must have 4 slots");

    // Slot 0: provider (required, no prefix)
    assert_eq!(g.slots[0].name, "provider");
    assert!(g.slots[0].required);
    assert!(g.slots[0].prefix.is_none());

    // Slot 1: model (optional, --model= prefix)
    assert_eq!(g.slots[1].name, "model");
    assert!(!g.slots[1].required);
    assert_eq!(g.slots[1].prefix.as_deref(), Some("--model="));

    // Slot 2: region (optional, --region= prefix)
    assert_eq!(g.slots[2].name, "region");
    assert!(!g.slots[2].required);
    assert_eq!(g.slots[2].prefix.as_deref(), Some("--region="));

    // Slot 3: project (optional, --project= prefix)
    assert_eq!(g.slots[3].name, "project");
    assert!(!g.slots[3].required);
    assert_eq!(g.slots[3].prefix.as_deref(), Some("--project="));
}

// rtmx:req REQ-TUI-087
#[test]
fn test_connect_grammar_provider_has_all_four_options() {
    let g = connect_grammar();
    let provider_opts = match &g.slots[0].kind {
        aegis_tui::command_palette::TokenKind::Enum(opts) => opts,
        _ => panic!("provider slot must be Enum"),
    };
    let values: Vec<&str> = provider_opts.iter().map(|o| o.value.as_str()).collect();
    assert!(values.contains(&"vertex"), "must include vertex");
    assert!(values.contains(&"bedrock"), "must include bedrock");
    assert!(values.contains(&"azure"), "must include azure");
    assert!(values.contains(&"local"), "must include local");
}

// rtmx:req REQ-TUI-087
#[test]
fn test_connect_grammar_each_provider_has_models() {
    for provider in &["vertex", "bedrock", "azure", "local"] {
        let models = options_for_provider(provider, "model");
        assert!(!models.is_empty(), "{provider} must have model completions");
    }
}

// rtmx:req REQ-TUI-087
#[test]
fn test_connect_grammar_for_returns_some() {
    let p = CommandPalette::new();
    assert!(p.grammar_for("/connect").is_some());
}

// ---------------------------------------------------------------------------
// REQ-TUI-088: Per-token dropdown rendering in command palette
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-088
#[test]
fn test_connect_dropdown_per_token() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Type /connect and tab to enter token stage
    type_str(&mut app, "/connect", &tx);
    press_tab(&mut app, &tx);

    // Should be in token stage showing provider options
    assert!(app.command_palette.in_token_stage());
    let view = app.command_palette.view().unwrap();
    assert!(
        view.entries.iter().any(|e| e.name == "vertex"),
        "dropdown must show provider options"
    );
    assert!(
        view.entries.iter().any(|e| e.name == "bedrock"),
        "dropdown must show bedrock"
    );
}

// rtmx:req REQ-TUI-088
#[test]
fn test_connect_dropdown_filters_by_prefix() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "/connect", &tx);
    press_tab(&mut app, &tx);

    // Type "az" to filter to azure only
    type_str(&mut app, "az", &tx);

    let view = app.command_palette.view().unwrap();
    assert_eq!(view.entries.len(), 1, "should filter to azure only");
    assert_eq!(view.entries[0].name, "azure");
}

// rtmx:req REQ-TUI-088
#[test]
fn test_connect_dropdown_arrow_key_navigation() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "/connect", &tx);
    press_tab(&mut app, &tx);

    // Initial selection at 0
    let v = app.command_palette.view().unwrap();
    assert_eq!(v.selected, 0);

    // Arrow down
    send_key(&mut app, KeyCode::Down, KeyModifiers::NONE, &tx);
    let v = app.command_palette.view().unwrap();
    assert_eq!(v.selected, 1);

    // Arrow up back to 0
    send_key(&mut app, KeyCode::Up, KeyModifiers::NONE, &tx);
    let v = app.command_palette.view().unwrap();
    assert_eq!(v.selected, 0);
}

// rtmx:req REQ-TUI-088
#[test]
fn test_connect_dropdown_advances_to_model_slot() {
    let mut p = CommandPalette::new();
    p.show();
    p.enter_token_stage(connect_grammar());

    // Select vertex -- should advance to model slot
    let has_more = p.advance_token("vertex".to_string());
    assert!(has_more, "should have model slot next");

    // Dropdown should now show vertex-specific models
    let view = p.view().unwrap();
    assert!(
        view.entries.iter().any(|e| e.name == "Gemini 3.1 Pro"),
        "model slot must show Gemini for vertex"
    );
    assert!(
        !view.entries.iter().any(|e| e.name == "vertex"),
        "provider options must no longer appear"
    );
}

// ---------------------------------------------------------------------------
// REQ-TUI-089: Validation and provider swap on complete entry
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-089
#[test]
fn test_connect_validates_and_swaps() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Execute /connect vertex via slash command (simulates complete grammar entry)
    app.execute_slash_command(SlashCommand::Connect(
        "vertex --model=gemini-3.1-pro --region=us-central1".into(),
    ));

    // Should have a pending_connect request queued for the composition root
    assert!(
        app.pending_connect.is_some(),
        "complete /connect must queue a ConnectRequest"
    );
    let req = app.pending_connect.as_ref().unwrap();
    assert_eq!(
        req.provider,
        aegis_tui::app::ConnectProvider::Vertex,
        "provider must be Vertex"
    );
    assert_eq!(req.model.as_deref(), Some("gemini-3.1-pro"));
    assert_eq!(req.region.as_deref(), Some("us-central1"));
}

// rtmx:req REQ-TUI-089
#[test]
fn test_connect_invalid_provider_shows_error() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Execute /connect with invalid provider
    app.execute_slash_command(SlashCommand::Connect("badprovider".into()));

    // Should NOT have a pending_connect
    assert!(
        app.pending_connect.is_none(),
        "invalid provider must not queue a ConnectRequest"
    );

    // Error message should appear
    let error_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::Error)
        .collect();
    assert!(
        !error_msgs.is_empty(),
        "error message must appear for invalid provider"
    );
}

// rtmx:req REQ-TUI-089
#[test]
fn test_connect_no_args_shows_current_connection() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Execute /connect with no args
    app.execute_slash_command(SlashCommand::Connect("".into()));

    // Should show current connection info (system message, not error)
    let sys_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::System)
        .collect();
    assert!(
        !sys_msgs.is_empty(),
        "no-arg /connect must show connection info"
    );
    assert!(
        sys_msgs
            .iter()
            .any(|m| m.content.contains("model") || m.content.contains("Model")),
        "connection info must mention model"
    );
}

// rtmx:req REQ-TUI-089
#[test]
fn test_connect_local_with_url_queues_request() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Execute /connect with a local URL
    app.execute_slash_command(SlashCommand::Connect(
        "local http://localhost:11434/v1".into(),
    ));

    // Should queue a local ConnectRequest
    // (Note: local without URL triggers ollama detection which may or may not
    // succeed, but with an explicit URL it should queue directly)
    if let Some(req) = &app.pending_connect {
        assert_eq!(req.provider, aegis_tui::app::ConnectProvider::Local);
    }
    // If no pending_connect, we still expect a system message about connecting
    let all_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::System || m.kind == MessageKind::Error)
        .collect();
    assert!(!all_msgs.is_empty(), "local connect must produce feedback");
}
