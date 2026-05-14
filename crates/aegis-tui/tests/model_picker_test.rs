//! Tests for the /model picker dropdown (REQ-TUI-090, REQ-TUI-091, REQ-TUI-092).

use aegis_tui::app::{Action, App, AppPhase};
use aegis_tui::command_palette::{CommandPalette, TokenOption, model_grammar};
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
    let event = aegis_tui::event::TuiEvent::Terminal(CtEvent::Key(KeyEvent::new(code, mods)));
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

fn inject_test_models(app: &mut App) {
    let models = vec![
        TokenOption {
            value: "llama3".into(),
            label: "llama3".into(),
            description: "available".into(),
        },
        TokenOption {
            value: "codellama".into(),
            label: "codellama".into(),
            description: "available".into(),
        },
        TokenOption {
            value: "mistral".into(),
            label: "mistral".into(),
            description: "available (current)".into(),
        },
    ];
    app.command_palette.inject_options("model", models);
}

// ---------------------------------------------------------------------------
// REQ-TUI-090: /model queries provider and populates dropdown
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-090
#[test]
fn test_model_grammar_has_single_model_slot() {
    let g = model_grammar();
    assert_eq!(g.name, "/model");
    assert_eq!(g.slots.len(), 1);
    assert_eq!(g.slots[0].name, "model");
    assert!(g.slots[0].required);
}

// rtmx:req REQ-TUI-090
#[test]
fn test_grammar_for_model_returns_some() {
    let p = CommandPalette::new();
    assert!(p.grammar_for("/model").is_some());
}

// rtmx:req REQ-TUI-090
#[test]
fn test_model_picker_populates_from_provider() {
    let mut app = App::new("mistral");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Type /model and tab-complete to enter token stage
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    // Should be in token stage showing loading state
    assert!(app.command_palette.in_token_stage());

    // Inject models as if discovery completed
    inject_test_models(&mut app);
    app.command_palette.refresh_current_slot();

    // Dropdown should now show the injected models
    let view = app.command_palette.view();
    assert!(view.is_some());
    let v = view.unwrap();
    assert!(v.entries.iter().any(|e| e.name == "llama3"));
    assert!(v.entries.iter().any(|e| e.name == "codellama"));
    assert!(v.entries.iter().any(|e| e.name == "mistral"));
}

// rtmx:req REQ-TUI-090
#[test]
fn test_model_discovery_triggers_pending_flag() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    assert!(!app.pending_model_discovery);

    // Type /model and tab-complete
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    assert!(
        app.pending_model_discovery,
        "entering /model token stage must set pending_model_discovery"
    );
}

// rtmx:req REQ-TUI-090
#[test]
fn test_models_ready_event_populates_dropdown() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Enter /model token stage
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    // Simulate ModelsReady event
    let event = TuiEvent::ModelsReady {
        models: vec![
            ("llama3".into(), "available".into()),
            ("codellama".into(), "available".into()),
        ],
    };
    app.handle_event(event, &tx);

    // Verify dropdown populated. The ModelsReady handler merges
    // discovery results with the provider catalog (default: "local"
    // which has Llama 3, Code Llama, Mistral). Discovery returned 2
    // models; the catalog adds Mistral for a total of 3.
    let view = app.command_palette.view();
    assert!(view.is_some());
    let v = view.unwrap();
    assert_eq!(v.entries.len(), 3);
    // Entry names use the label (e.g. "Llama 3"), not the value ("llama3")
    assert!(v.entries.iter().any(|e| e.name == "Llama 3"));
    assert!(v.entries.iter().any(|e| e.name == "Code Llama"));
    assert!(v.entries.iter().any(|e| e.name == "Mistral"));
}

// rtmx:req REQ-TUI-090
#[test]
fn test_models_error_event_shows_fallback() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Enter /model token stage
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    // Simulate ModelsError event
    let event = TuiEvent::ModelsError {
        message: "connection refused".into(),
    };
    app.handle_event(event, &tx);

    // Verify error message shown
    let sys_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::System)
        .collect();
    assert!(
        sys_msgs
            .iter()
            .any(|m| m.content.contains("connection refused")),
        "error message should appear in chat"
    );

    // Verify manual fallback option
    let view = app.command_palette.view();
    assert!(view.is_some());
    let v = view.unwrap();
    assert!(v.entries.iter().any(|e| e.name.contains("manually")));
}

// ---------------------------------------------------------------------------
// REQ-TUI-091: Arrow-key selectable dropdown with current model highlighted
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-091
#[test]
fn test_model_picker_arrow_key_selection() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Enter /model token stage and inject models
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);
    inject_test_models(&mut app);
    app.command_palette.refresh_current_slot();

    // Initial selection should be 0
    let v = app.command_palette.view().unwrap();
    assert_eq!(v.selected, 0);

    // Arrow down
    send_key(&mut app, KeyCode::Down, KeyModifiers::NONE, &tx);
    let v = app.command_palette.view().unwrap();
    assert_eq!(v.selected, 1);

    // Arrow down again
    send_key(&mut app, KeyCode::Down, KeyModifiers::NONE, &tx);
    let v = app.command_palette.view().unwrap();
    assert_eq!(v.selected, 2);

    // Arrow up
    send_key(&mut app, KeyCode::Up, KeyModifiers::NONE, &tx);
    let v = app.command_palette.view().unwrap();
    assert_eq!(v.selected, 1);
}

// rtmx:req REQ-TUI-091
#[test]
fn test_model_picker_current_model_highlighted() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Enter /model token stage
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    // Simulate ModelsReady with current model
    let event = TuiEvent::ModelsReady {
        models: vec![
            ("llama3".into(), "available".into()),
            ("codellama".into(), "available".into()),
        ],
    };
    app.handle_event(event, &tx);

    // Current model should have "(current)" in description.
    // Entry names use the catalog label, not the raw model ID.
    let view = app.command_palette.view().unwrap();
    let current = view.entries.iter().find(|e| e.name == "Llama 3").unwrap();
    assert!(
        current.description.contains("(current)"),
        "current model must be marked: {}",
        current.description
    );
    // Non-current model should not have "(current)"
    let other = view
        .entries
        .iter()
        .find(|e| e.name == "Code Llama")
        .unwrap();
    assert!(
        !other.description.contains("(current)"),
        "non-current model must not be marked: {}",
        other.description
    );
}

// rtmx:req REQ-TUI-091
#[test]
fn test_model_picker_prefix_filter() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Enter /model token stage and inject models
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);
    inject_test_models(&mut app);
    app.command_palette.refresh_current_slot();

    // Type "mis" to filter -- should narrow to mistral only
    type_str(&mut app, "mis", &tx);

    let view = app.command_palette.view();
    assert!(view.is_some());
    let v = view.unwrap();
    assert_eq!(v.entries.len(), 1, "should filter to mistral only");
    assert_eq!(v.entries[0].name, "mistral");
}

// ---------------------------------------------------------------------------
// REQ-TUI-092: Validates selection before switching
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-092
#[test]
fn test_model_picker_validates_before_switch() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Set discovered models with one unavailable
    app.discovered_models = vec![
        ("llama3".into(), "available".into()),
        ("badmodel".into(), "unauthorized".into()),
    ];

    // Execute /model badmodel directly
    app.execute_slash_command(SlashCommand::Model("badmodel".into()));

    // Model should NOT have switched
    assert_eq!(
        app.model_name, "llama3",
        "model must not switch to unavailable"
    );

    // Error message should appear
    let error_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::Error)
        .collect();
    assert!(
        !error_msgs.is_empty(),
        "error message must appear for unavailable model"
    );
    assert!(
        error_msgs[0].content.contains("unauthorized"),
        "error must show status: {}",
        error_msgs[0].content
    );
}

// rtmx:req REQ-TUI-092
#[test]
fn test_model_picker_switches_on_valid_selection() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Set discovered models
    app.discovered_models = vec![
        ("llama3".into(), "available".into()),
        ("codellama".into(), "available".into()),
    ];

    // Execute /model codellama directly
    app.execute_slash_command(SlashCommand::Model("codellama".into()));

    assert_eq!(
        app.model_name, "codellama",
        "model must switch to available model"
    );
}

// rtmx:req REQ-TUI-092
#[test]
fn test_model_picker_skips_validation_without_discovery() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // No discovered models -- validation should be skipped
    assert!(app.discovered_models.is_empty());

    // Execute /model newmodel directly
    app.execute_slash_command(SlashCommand::Model("newmodel".into()));

    assert_eq!(
        app.model_name, "newmodel",
        "model switch must work without discovery"
    );
}
