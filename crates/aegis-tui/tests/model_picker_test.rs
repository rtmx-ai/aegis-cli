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
            ("llama3".into(), "available".into(), Some("US".into())),
            ("codellama".into(), "available".into(), Some("US".into())),
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
            ("llama3".into(), "available".into(), Some("US".into())),
            ("codellama".into(), "available".into(), Some("US".into())),
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
        ("llama3".into(), "available".into(), Some("US".into())),
        ("badmodel".into(), "unauthorized".into(), None),
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
        ("llama3".into(), "available".into(), Some("US".into())),
        ("codellama".into(), "available".into(), Some("US".into())),
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

// ---------------------------------------------------------------------------
// REQ-TUI-106: Model picker shows origin country and policy tier
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-106
#[test]
fn test_model_picker_shows_origin_column() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    // Enter /model token stage
    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    // Simulate ModelsReady with origin info
    let event = TuiEvent::ModelsReady {
        models: vec![
            ("llama3".into(), "available".into(), Some("US".into())),
            ("mistral".into(), "available".into(), Some("France".into())),
        ],
    };
    app.handle_event(event, &tx);

    let view = app.command_palette.view().unwrap();

    // Llama3 should show [US] in description
    let llama = view.entries.iter().find(|e| e.name == "Llama 3").unwrap();
    assert!(
        llama.description.contains("[US]"),
        "US origin must appear: {}",
        llama.description
    );

    // Mistral should show [France] in description
    let mistral = view.entries.iter().find(|e| e.name == "Mistral").unwrap();
    assert!(
        mistral.description.contains("[France]"),
        "France origin must appear: {}",
        mistral.description
    );
}

// rtmx:req REQ-TUI-106
#[test]
fn test_model_picker_origin_with_current_marker() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    let event = TuiEvent::ModelsReady {
        models: vec![("llama3".into(), "available".into(), Some("US".into()))],
    };
    app.handle_event(event, &tx);

    let view = app.command_palette.view().unwrap();
    let llama = view.entries.iter().find(|e| e.name == "Llama 3").unwrap();
    assert!(
        llama.description.contains("[US]") && llama.description.contains("(current)"),
        "origin and current marker must both appear: {}",
        llama.description
    );
}

// ---------------------------------------------------------------------------
// REQ-TUI-107: Restricted models shown grayed with denial reason
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-107
#[test]
fn test_restricted_model_grayed_with_reason() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    let (tx, _rx) = agent_tx();

    type_str(&mut app, "/model", &tx);
    press_tab(&mut app, &tx);

    // Simulate a restricted model from origin policy
    let event = TuiEvent::ModelsReady {
        models: vec![
            ("llama3".into(), "available".into(), Some("US".into())),
            ("qwen".into(), "restricted".into(), Some("China".into())),
        ],
    };
    app.handle_event(event, &tx);

    let view = app.command_palette.view().unwrap();

    // Qwen should appear in the list (not hidden) with restricted status
    let qwen = view.entries.iter().find(|e| e.name == "qwen");
    assert!(qwen.is_some(), "restricted model must be visible in picker");
    let qwen = qwen.unwrap();
    assert!(
        qwen.description.contains("restricted"),
        "restricted status must appear: {}",
        qwen.description
    );
    assert!(
        qwen.description.contains("[China]"),
        "origin must appear for restricted model: {}",
        qwen.description
    );
}

// rtmx:req REQ-TUI-107
#[test]
fn test_restricted_model_cannot_be_selected() {
    use aegis_tui::slash_commands::SlashCommand;

    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Set discovered models with restricted model
    app.discovered_models = vec![
        ("llama3".into(), "available".into(), Some("US".into())),
        ("qwen".into(), "restricted".into(), Some("China".into())),
    ];

    // Try to switch to restricted model
    app.execute_slash_command(SlashCommand::Model("qwen".into()));

    // Model must NOT switch
    assert_eq!(
        app.model_name, "llama3",
        "must not switch to restricted model"
    );

    // Error message should mention restriction
    let errors: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::Error)
        .collect();
    assert!(
        !errors.is_empty(),
        "error message must appear for restricted model"
    );
    assert!(
        errors[0].content.contains("restricted"),
        "error must mention restricted status: {}",
        errors[0].content
    );
}

// ---------- REQ-TUI-108: /model download tests ----------

// rtmx:req REQ-TUI-108
#[test]
fn test_model_download_command_sets_pending() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Parse and execute /model download gemma4
    let cmd = aegis_tui::slash_commands::parse_slash_command("/model download gemma4");
    if let aegis_tui::slash_commands::ParseResult::Command(c) = cmd {
        app.execute_slash_command(c);
    } else {
        panic!("should parse as command");
    }

    assert_eq!(
        app.pending_model_download.as_deref(),
        Some("gemma4"),
        "pending download should be set"
    );
    assert_eq!(
        app.active_download.as_deref(),
        Some("gemma4"),
        "active download should be set"
    );
    // Should have a system message about downloading
    let sys_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::System)
        .collect();
    assert!(
        sys_msgs.iter().any(|m| m.content.contains("Downloading")),
        "should show downloading message"
    );
}

// rtmx:req REQ-TUI-108
#[test]
fn test_model_download_denied_model_shows_error() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    // Mark qwen:7b as restricted in discovered models
    app.discovered_models = vec![(
        "qwen:7b".to_string(),
        "restricted".to_string(),
        Some("China".to_string()),
    )];

    let cmd = aegis_tui::slash_commands::parse_slash_command("/model download qwen:7b");
    if let aegis_tui::slash_commands::ParseResult::Command(c) = cmd {
        app.execute_slash_command(c);
    } else {
        panic!("should parse as command");
    }

    assert!(
        app.pending_model_download.is_none(),
        "denied model should not trigger download"
    );
    let errors: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::Error)
        .collect();
    assert!(!errors.is_empty(), "should show error for denied model");
    assert!(
        errors[0].content.contains("Cannot download"),
        "error should say cannot download: {}",
        errors[0].content
    );
}

// rtmx:req REQ-TUI-108
#[test]
fn test_model_download_no_name_shows_usage() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;

    let cmd = aegis_tui::slash_commands::parse_slash_command("/model download");
    if let aegis_tui::slash_commands::ParseResult::Command(c) = cmd {
        app.execute_slash_command(c);
    } else {
        panic!("should parse as command");
    }

    assert!(
        app.pending_model_download.is_none(),
        "empty name should not trigger download"
    );
    let errors: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::Error)
        .collect();
    assert!(
        !errors.is_empty(),
        "should show usage error for empty download name"
    );
    assert!(
        errors[0].content.contains("Usage"),
        "error should show usage: {}",
        errors[0].content
    );
}

// rtmx:req REQ-TUI-108
#[test]
fn test_model_download_complete_triggers_rediscovery() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    app.active_download = Some("gemma4".to_string());
    let (tx, _rx) = agent_tx();

    let action = app.handle_event(
        TuiEvent::ModelDownloadComplete {
            model: "gemma4".to_string(),
        },
        &tx,
    );

    assert!(matches!(action, Action::Continue));
    assert!(app.active_download.is_none(), "download should be cleared");
    assert!(
        app.pending_model_discovery,
        "should trigger model re-discovery after download"
    );
    let sys_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::System)
        .collect();
    assert!(
        sys_msgs.iter().any(|m| m.content.contains("downloaded")),
        "should show download complete message"
    );
}

// rtmx:req REQ-TUI-108
#[test]
fn test_model_download_failed_shows_error() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    app.active_download = Some("gemma4".to_string());
    let (tx, _rx) = agent_tx();

    let action = app.handle_event(
        TuiEvent::ModelDownloadFailed {
            model: "gemma4".to_string(),
            reason: "connection refused".to_string(),
        },
        &tx,
    );

    assert!(matches!(action, Action::Continue));
    assert!(app.active_download.is_none(), "download should be cleared");
    let errors: Vec<_> = app
        .messages
        .iter()
        .filter(|m| m.kind == MessageKind::Error)
        .collect();
    assert!(!errors.is_empty(), "should show error on download failure");
    assert!(
        errors[0].content.contains("connection refused"),
        "error should include reason: {}",
        errors[0].content
    );
}

// rtmx:req REQ-TUI-108
#[test]
fn test_model_download_progress_updates_message() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    app.active_download = Some("gemma4".to_string());
    let (tx, _rx) = agent_tx();

    // Add initial system message (like "Downloading model...")
    app.messages.push(aegis_tui::messages::ChatMessage::system(
        "Downloading model 'gemma4'...".to_string(),
    ));

    let action = app.handle_event(
        TuiEvent::ModelDownloadProgress {
            model: "gemma4".to_string(),
            status: "downloading".to_string(),
            completed: 524_288_000, // 500 MB
            total: 1_048_576_000,   // 1000 MB
        },
        &tx,
    );

    assert!(matches!(action, Action::Continue));
    // The last system message should be updated with progress
    let last = app.messages.last().unwrap();
    assert_eq!(last.kind, MessageKind::System);
    assert!(
        last.content.contains("500"),
        "should show completed MB: {}",
        last.content
    );
    assert!(
        last.content.contains("50%"),
        "should show percentage: {}",
        last.content
    );
}

// ---------- REQ-TUI-109: Download progress bar tests ----------

// rtmx:req REQ-TUI-109
#[test]
fn test_download_progress_updates_state() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    app.active_download = Some("gemma4".to_string());
    let (tx, _rx) = agent_tx();

    // First progress event should create DownloadProgress
    app.messages.push(aegis_tui::messages::ChatMessage::system(
        "starting...".to_string(),
    ));

    let _ = app.handle_event(
        TuiEvent::ModelDownloadProgress {
            model: "gemma4".to_string(),
            status: "downloading".to_string(),
            completed: 100_000_000,
            total: 500_000_000,
        },
        &tx,
    );

    let progress = app
        .download_progress
        .as_ref()
        .expect("should have progress");
    assert_eq!(progress.model, "gemma4");
    assert_eq!(progress.completed, 100_000_000);
    assert_eq!(progress.total, 500_000_000);
    assert_eq!(progress.percent(), 20);
}

// rtmx:req REQ-TUI-109
#[test]
fn test_download_progress_cleared_on_complete() {
    let mut app = App::new("llama3");
    app.phase = AppPhase::Idle;
    app.active_download = Some("gemma4".to_string());
    app.download_progress = Some(aegis_tui::app::DownloadProgress {
        model: "gemma4".to_string(),
        status: "downloading".to_string(),
        completed: 500_000_000,
        total: 500_000_000,
        started_at: std::time::Instant::now(),
    });
    let (tx, _rx) = agent_tx();

    let _ = app.handle_event(
        TuiEvent::ModelDownloadComplete {
            model: "gemma4".to_string(),
        },
        &tx,
    );

    assert!(
        app.download_progress.is_none(),
        "progress should be cleared"
    );
}

// rtmx:req REQ-TUI-109
#[test]
fn test_download_progress_label_format() {
    let progress = aegis_tui::app::DownloadProgress {
        model: "gemma4".to_string(),
        status: "downloading".to_string(),
        completed: 524_288_000, // 500 MB
        total: 1_048_576_000,   // 1000 MB
        started_at: std::time::Instant::now(),
    };

    let label = progress.label();
    assert!(label.contains("500"), "should show 500 MB done: {label}");
    assert!(label.contains("1000"), "should show 1000 MB total: {label}");
    assert!(label.contains("50%"), "should show 50%: {label}");
    assert!(label.contains("downloading"), "should show status: {label}");
}

// rtmx:req REQ-TUI-109
#[test]
fn test_download_progress_zero_total_shows_status() {
    let progress = aegis_tui::app::DownloadProgress {
        model: "gemma4".to_string(),
        status: "pulling manifest".to_string(),
        completed: 0,
        total: 0,
        started_at: std::time::Instant::now(),
    };

    let label = progress.label();
    assert_eq!(
        label, "pulling manifest",
        "zero total should show status only"
    );
    assert_eq!(progress.percent(), 0);
}
