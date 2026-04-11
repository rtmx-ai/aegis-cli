//! aegis-cli: Terminal-native agentic AI pair programmer for CUI
//! environments.

use aegis_domain::error::DomainError;
use aegis_domain::ports::{ApprovalGate, StreamEvent};
use aegis_domain::types::{ApprovalDecision, ToolCall};
use aegis_tui::app::{Action, App};
use aegis_tui::event::{ApprovalRequestHandle, TuiEvent};
use async_trait::async_trait;
use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

/// Initialize tracing to stderr (headless mode, init, doctor).
fn init_tracing_stderr() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
}

/// Initialize tracing to ~/.aegis/debug.log (TUI mode).
/// Returns the guard that must be held for the lifetime of the program.
fn init_tracing_file() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".aegis");

    // Ensure directory exists
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "debug.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    Some(guard)
}

/// Auto-approve gate for headless (non-interactive) mode.
struct HeadlessApprovalGate;

#[async_trait]
impl ApprovalGate for HeadlessApprovalGate {
    async fn request_approval(
        &self,
        _tool_call: &ToolCall,
    ) -> Result<ApprovalDecision, DomainError> {
        Ok(ApprovalDecision::Approved)
    }
}

/// Build version with embedded git SHA and target.
fn long_version() -> &'static str {
    let v = format!(
        "{} ({} {})",
        env!("CARGO_PKG_VERSION"),
        option_env!("AEGIS_GIT_SHA").unwrap_or("dev"),
        option_env!("AEGIS_TARGET").unwrap_or("native"),
    );
    Box::leak(v.into_boxed_str())
}

#[derive(Parser)]
#[command(
    name = "aegis",
    version = env!("CARGO_PKG_VERSION"),
    long_version = long_version(),
    about = "Agentic AI pair programmer for CUI environments"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize aegis with a cloud backend or local model
    Init {
        /// Configure for air-gapped operation with local models only
        #[arg(long)]
        local: bool,
    },
    /// Start an interactive chat session
    Chat {
        /// Initial prompt
        #[arg(short, long)]
        prompt: Option<String>,
        /// Run without TUI (for E2E testing and scripting)
        #[arg(long)]
        headless: bool,
        /// Override LLM endpoint (for testing)
        #[arg(long)]
        local_endpoint: Option<String>,
    },
    /// Check infrastructure health via plugin status
    Doctor,
}

fn main() {
    // Tracing is initialized per-mode: file appender for TUI, stderr for headless.
    // We defer init until we know the mode.
    let cli = Cli::parse();

    // Initialize tracing based on mode. TUI mode logs to file; everything else to stderr.
    // No-subcommand with existing config also launches TUI mode.
    let is_tui_mode = matches!(
        cli.command,
        Some(Commands::Chat {
            headless: false,
            ..
        })
    ) || (cli.command.is_none() && !needs_first_run_wizard());
    let _log_guard = if is_tui_mode {
        init_tracing_file()
    } else {
        init_tracing_stderr();
        None
    };

    let result = match cli.command {
        Some(Commands::Init { local }) => run_init(local),
        Some(Commands::Chat {
            prompt,
            headless,
            local_endpoint,
        }) => run_chat(prompt, headless, local_endpoint),
        Some(Commands::Doctor) => {
            eprintln!("aegis doctor: not yet implemented");
            std::process::exit(1);
        }
        None => {
            if needs_first_run_wizard() {
                // First run: launch the init wizard
                run_init(true)
            } else {
                // Config exists: launch interactive chat
                run_chat(None, false, None)
            }
        }
    };

    if let Err(e) = result {
        eprintln!("aegis: {e}");
        std::process::exit(1);
    }
}

/// Check whether the first-run wizard should be shown.
///
/// Returns `true` when no config file exists, meaning the user has
/// never completed `aegis init`. Used by the no-subcommand path to
/// decide between launching the wizard or starting interactive chat.
fn needs_first_run_wizard() -> bool {
    let config_path = aegis_onboard::config::AegisConfig::default_path().ok();
    match config_path {
        Some(path) => {
            let config_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            aegis_onboard::init::should_show_wizard(config_dir)
        }
        None => true, // Cannot determine home dir, assume first run
    }
}

fn run_init(local: bool) -> Result<(), String> {
    if !local {
        return Err("Cloud modes not yet implemented. \
             Use: aegis init --local"
            .to_string());
    }

    let inputs = aegis_onboard::init::InitInputs::local();
    let config_path =
        aegis_onboard::config::AegisConfig::default_path().map_err(|e| e.to_string())?;

    let result =
        aegis_onboard::init::run_init(&inputs, &config_path).map_err(|e| e.to_string())?;

    eprintln!("Configuration written to {}", result.config_path.display());
    eprintln!("Mode: {:?}. Backend: local (Ollama).", result.mode);
    eprintln!("Run 'aegis chat' to start a session.");
    Ok(())
}

fn run_chat(
    prompt: Option<String>,
    headless: bool,
    local_endpoint: Option<String>,
) -> Result<(), String> {
    if headless {
        let prompt = prompt.ok_or_else(|| {
            "Prompt required in headless mode. Use: aegis chat --headless -p 'your prompt'"
                .to_string()
        })?;

        let (endpoint, model) = resolve_endpoint_model(local_endpoint)?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
        return rt.block_on(async { run_headless_chat(&prompt, &endpoint, &model).await });
    }

    // Interactive TUI mode
    let (endpoint, model) = resolve_endpoint_model(local_endpoint)?;
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
    rt.block_on(async { run_interactive_chat(&endpoint, &model, prompt).await })
}

fn resolve_endpoint_model(local_endpoint: Option<String>) -> Result<(String, String), String> {
    if let Some(ep) = local_endpoint {
        return Ok((ep, "default".to_string()));
    }
    let config_path =
        aegis_onboard::config::AegisConfig::default_path().map_err(|e| e.to_string())?;
    let config = aegis_onboard::config::AegisConfig::load(&config_path)
        .map_err(|e| format!("No config found. Run 'aegis init --local' first. ({e})"))?;
    Ok((config.backend.endpoint, config.backend.model))
}

async fn run_headless_chat(prompt: &str, endpoint: &str, model: &str) -> Result<(), String> {
    // 1. Create LLM provider (local mode)
    let provider_config = aegis_llm::config::ProviderConfig::local(endpoint, model);
    let provider =
        aegis_llm::local::LocalProvider::new(&provider_config).map_err(|e| e.to_string())?;

    // 2. Create security filter
    let filter: Arc<aegis_security::aegisignore::AegisIgnore> =
        Arc::new(aegis_security::aegisignore::AegisIgnore::with_defaults());

    // 3. Create tool executor
    let work_dir = std::env::current_dir().map_err(|e| format!("Cannot get cwd: {e}"))?;
    let executor = aegis_agent::tools::BuiltinExecutor::new(filter.clone(), &work_dir);

    // 4. Create HITL gate (auto-approve in headless)
    let gate = HeadlessApprovalGate;

    // 5. Create audit ledger
    let log_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".aegis/logs");
    let ledger = aegis_audit::ledger::JsonlLedger::new(&log_dir)
        .await
        .map_err(|e| e.to_string())?;

    // 6. Run the agent loop
    let config = aegis_agent::loop_runner::AgentConfig {
        max_iterations: 20,
        system_prompt: "You are a helpful coding assistant. \
             You have access to tools: read_file, \
             write_file, run_command, list_dir, grep."
            .to_string(),
    };

    let agent = aegis_agent::loop_runner::AgentLoop::new(
        provider, gate, executor, ledger, filter, config,
    );

    let result = agent.run(prompt).await.map_err(|e| e.to_string())?;

    // Print result to stdout (headless output)
    println!("{}", result.response);
    eprintln!(
        "[{} iterations, {}in + {}out tokens]",
        result.iterations, result.input_tokens, result.output_tokens
    );

    Ok(())
}

async fn run_interactive_chat(
    endpoint: &str,
    model: &str,
    initial_prompt: Option<String>,
) -> Result<(), String> {
    use crossterm::event as ct_event;
    use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
    use ratatui::Terminal;
    use ratatui::backend::CrosstermBackend;
    use tokio::sync::mpsc;

    // 1. Set up terminal
    crossterm::terminal::enable_raw_mode().map_err(|e| format!("Terminal error: {e}"))?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableMouseCapture,
        crossterm::cursor::SetCursorStyle::SteadyBlock
    )
    .map_err(|e| format!("Terminal error: {e}"))?;

    // Enable Kitty keyboard protocol if the terminal supports it.
    // This allows Shift+Enter to be distinguished from Enter.
    let enhanced_keyboard = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced_keyboard {
        crossterm::execute!(
            stdout,
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        )
        .map_err(|e| format!("Keyboard enhancement error: {e}"))?;
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| format!("Terminal error: {e}"))?;

    // 2. Create unified event channel
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<TuiEvent>();

    // 3. Create agent input channel (prompts from TUI -> agent task)
    let (agent_input_tx, mut agent_input_rx) = mpsc::unbounded_channel::<String>();

    // 4. Create HITL approval channel
    let (approval_gate, mut approval_rx) = aegis_hitl::approval::create_approval_channel(4);

    // 5. Spawn crossterm event reader on a dedicated OS thread
    let event_tx_term = event_tx.clone();
    std::thread::spawn(move || {
        loop {
            if ct_event::poll(Duration::from_millis(50)).unwrap_or(false)
                && let Ok(evt) = ct_event::read()
                && event_tx_term.send(TuiEvent::Terminal(evt)).is_err()
            {
                break;
            }
        }
    });

    // 6. Spawn tick timer for animations
    let event_tx_tick = event_tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(150));
        loop {
            interval.tick().await;
            if event_tx_tick.send(TuiEvent::Tick).is_err() {
                break;
            }
        }
    });

    // 7. Spawn HITL approval forwarder
    let event_tx_hitl = event_tx.clone();
    tokio::spawn(async move {
        while let Some(req) = approval_rx.recv().await {
            let handle = ApprovalRequestHandle {
                tool_call: req.tool_call,
                description: req.description,
                response_tx: req.response_tx,
            };
            if event_tx_hitl
                .send(TuiEvent::ApprovalRequest(handle))
                .is_err()
            {
                break;
            }
        }
    });

    // 8. Spawn agent task (listens for prompts, runs agent, forwards stream events)
    let endpoint_owned = endpoint.to_string();
    let model_owned = model.to_string();
    let event_tx_agent = event_tx.clone();
    tokio::spawn(async move {
        while let Some(prompt) = agent_input_rx.recv().await {
            let result = run_agent_for_tui(
                &prompt,
                &endpoint_owned,
                &model_owned,
                approval_gate.clone(),
                &event_tx_agent,
            )
            .await;
            if let Err(e) = result {
                let _ = event_tx_agent.send(TuiEvent::AgentError(e));
            }
        }
    });

    // 9. Detect platform (immutable for session lifetime)
    let platform = aegis_tui::platform::Platform::detect();

    // 10. Create App state, restoring previous session if available (REQ-BUILD-036)
    let mut app = App::new(model);
    let session_dir = aegis_agent::session::default_session_dir();
    if let Some(ref dir) = session_dir {
        let current = dir.join("current.json");
        if let Some(snapshot) = aegis_agent::session::load_session(&current) {
            tracing::info!(
                session_id = %snapshot.session_id,
                messages = snapshot.messages.len(),
                "restoring session from snapshot"
            );
            restore_app_from_snapshot(&mut app, &snapshot);
        }
    }
    let session_id = aegis_domain::types::SessionId::new().to_string();
    let work_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // 10. If initial prompt provided, submit it immediately
    if let Some(prompt) = initial_prompt {
        app.messages
            .push(aegis_tui::messages::ChatMessage::user(&prompt));
        app.phase = aegis_tui::app::AppPhase::Streaming;
        let _ = agent_input_tx.send(prompt);
    }

    // SIGTERM future for graceful save-on-exit (REQ-BUILD-035)
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| format!("signal handler: {e}"))?;

    // 11. Event loop
    loop {
        // Render current state
        terminal
            .draw(|frame| {
                if app.phase == aegis_tui::app::AppPhase::Splash {
                    aegis_tui::splash::render_splash(frame, frame.area());
                } else {
                    let input_mode = match app.input.mode {
                        aegis_tui::input::InputMode::Insert => {
                            aegis_tui::layout::InputModeDisplay::Insert
                        }
                        aegis_tui::input::InputMode::Normal => {
                            aegis_tui::layout::InputModeDisplay::Normal
                        }
                    };
                    let view = aegis_tui::layout::AppState {
                        messages: app.messages.clone(),
                        input: app.input.text.clone(),
                        cursor: app.input.cursor,
                        status: app.status_info(),
                        scroll_offset: app.scroll_offset,
                        input_mode,
                        newline_hint: platform.newline_hint.to_string(),
                        stream_buffer: app.stream_buffer.clone(),
                        approval_display: app.approval_display.clone(),
                    };
                    aegis_tui::layout::render(frame, &view);
                }
            })
            .map_err(|e| format!("Render error: {e}"))?;

        // Wait for next event OR SIGTERM
        #[cfg(unix)]
        let next_event = tokio::select! {
            evt = event_rx.recv() => evt,
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received, saving session and exiting");
                None
            }
        };
        #[cfg(not(unix))]
        let next_event = event_rx.recv().await;

        match next_event {
            Some(event) => {
                if app.handle_event(event, &agent_input_tx) == Action::Quit {
                    break;
                }
            }
            None => break,
        }
    }

    // 12. Save session before cleanup (REQ-BUILD-035)
    if let Some(ref dir) = session_dir
        && !app.messages.is_empty()
    {
        let snapshot = build_snapshot_from_app(&app, &session_id, work_dir.clone());
        match aegis_agent::session::save_session(dir, &snapshot) {
            Ok(path) => tracing::info!(path = %path.display(), "session saved"),
            Err(e) => tracing::warn!(%e, "failed to save session"),
        }
    }

    // 13. Cleanup terminal
    if enhanced_keyboard {
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::event::PopKeyboardEnhancementFlags
        )
        .ok();
    }
    crossterm::terminal::disable_raw_mode().ok();
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::cursor::SetCursorStyle::DefaultUserShape,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
        LeaveAlternateScreen
    )
    .ok();
    terminal.show_cursor().ok();

    Ok(())
}

/// Build a SessionSnapshot from the current App state.
/// Converts TUI ChatMessage values into domain Message values for persistence.
fn build_snapshot_from_app(
    app: &App,
    session_id: &str,
    work_dir: std::path::PathBuf,
) -> aegis_agent::session::SessionSnapshot {
    use aegis_domain::ports::{Message, Role};
    use aegis_tui::messages::MessageKind;

    let messages: Vec<Message> = app
        .messages
        .iter()
        .filter_map(|m| {
            let role = match m.kind {
                MessageKind::User => Some(Role::User),
                MessageKind::Assistant => Some(Role::Assistant),
                // Tool calls/results, errors, system messages are not persisted
                // as conversation history; the LLM rebuilds context from User/Assistant only.
                _ => None,
            };
            role.map(|r| Message {
                role: r,
                content: m.content.clone(),
            })
        })
        .collect();

    aegis_agent::session::SessionSnapshot::new(
        session_id.to_string(),
        messages,
        app.input_tokens,
        app.output_tokens,
        app.model_name.clone(),
        work_dir,
    )
}

/// Restore App state from a SessionSnapshot.
/// Inverse of build_snapshot_from_app: converts domain Message back into TUI ChatMessage.
fn restore_app_from_snapshot(app: &mut App, snapshot: &aegis_agent::session::SessionSnapshot) {
    use aegis_domain::ports::Role;
    use aegis_tui::messages::ChatMessage;

    app.messages = snapshot
        .messages
        .iter()
        .map(|m| match m.role {
            Role::User => ChatMessage::user(m.content.clone()),
            Role::Assistant => ChatMessage::assistant(m.content.clone()),
            _ => ChatMessage::system(m.content.clone()),
        })
        .collect();
    app.input_tokens = snapshot.input_tokens;
    app.output_tokens = snapshot.output_tokens;
    if !snapshot.messages.is_empty() {
        // Skip splash when resuming a session
        app.phase = aegis_tui::app::AppPhase::Idle;
        app.messages.push(ChatMessage::system(format!(
            "(restored {} messages from previous session)",
            snapshot.messages.len()
        )));
    }
}

/// Run the agent loop for one prompt, forwarding stream events to the TUI.
async fn run_agent_for_tui(
    prompt: &str,
    endpoint: &str,
    model: &str,
    gate: aegis_hitl::approval::ChannelApprovalGate,
    event_tx: &tokio::sync::mpsc::UnboundedSender<TuiEvent>,
) -> Result<(), String> {
    let provider_config = aegis_llm::config::ProviderConfig::local(endpoint, model);
    let provider =
        aegis_llm::local::LocalProvider::new(&provider_config).map_err(|e| e.to_string())?;

    let filter: Arc<aegis_security::aegisignore::AegisIgnore> =
        Arc::new(aegis_security::aegisignore::AegisIgnore::with_defaults());
    let work_dir = std::env::current_dir().map_err(|e| format!("Cannot get cwd: {e}"))?;
    let executor = aegis_agent::tools::BuiltinExecutor::new(filter.clone(), &work_dir);

    let log_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".aegis/logs");
    let ledger = aegis_audit::ledger::JsonlLedger::new(&log_dir)
        .await
        .map_err(|e| e.to_string())?;

    let config = aegis_agent::loop_runner::AgentConfig {
        max_iterations: 20,
        system_prompt: "You are a helpful coding assistant. \
             You have access to tools: read_file, \
             write_file, run_command, list_dir, grep."
            .to_string(),
    };

    // Create a stream event sink that translates StreamEvents into TuiEvents
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();
    let event_tx_stream = event_tx.clone();
    tokio::spawn(async move {
        while let Some(se) = stream_rx.recv().await {
            let tui_event = match se {
                StreamEvent::Token(text) => TuiEvent::AgentToken(text),
                StreamEvent::ToolUse(call) => TuiEvent::AgentToolUse(call),
                StreamEvent::Done {
                    input_tokens,
                    output_tokens,
                } => TuiEvent::AgentDone {
                    input_tokens,
                    output_tokens,
                },
                StreamEvent::Error(msg) => TuiEvent::AgentError(msg),
            };
            if event_tx_stream.send(tui_event).is_err() {
                break;
            }
        }
    });

    let agent = aegis_agent::loop_runner::AgentLoop::new(
        provider, gate, executor, ledger, filter, config,
    )
    .with_event_sink(stream_tx);

    agent.run(prompt).await.map_err(|e| e.to_string())?;
    Ok(())
}
