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

/// Build the assembled system prompt from base identity + repo context.
///
/// Uses the SystemPromptManager layering: Base (aegis identity shipped with
/// binary) + Project (repo context gathered from cwd). Keeps the model
/// aware of its mission, tools, security posture, and current project state.
fn build_system_prompt() -> String {
    use aegis_agent::system_prompt::{SystemPromptLayer, SystemPromptManager};
    let mut mgr = SystemPromptManager::with_base();
    let work_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo_ctx = aegis_agent::repo_context::RepoContext::gather(&work_dir);
    let project_section = repo_ctx.to_prompt_section();
    if !project_section.trim().is_empty() {
        mgr.set_layer(SystemPromptLayer::Project, project_section);
    }
    mgr.build()
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
        /// Plain-text interactive mode (no TUI, screen-reader friendly)
        #[arg(long)]
        no_tui: bool,
        /// Override LLM endpoint (for testing)
        #[arg(long)]
        local_endpoint: Option<String>,
        /// LLM provider: local, vertex, bedrock, azure
        #[arg(long)]
        provider: Option<String>,
        /// Model name or deployment ID
        #[arg(long)]
        model: Option<String>,
        /// Cloud region (required for Bedrock, optional for Vertex)
        #[arg(long)]
        region: Option<String>,
        /// Provider endpoint URL (overrides default)
        #[arg(long)]
        endpoint: Option<String>,
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
            no_tui: false,
            ..
        })
    ) || (cli.command.is_none() && !needs_first_run_wizard());
    // Plain-text and headless modes use stderr; auto-detect NO_COLOR/TERM=dumb
    let is_tui_mode = is_tui_mode && !aegis_tui::terminal::should_use_plain_text(false);
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
            no_tui,
            local_endpoint,
            provider,
            model,
            region,
            endpoint,
        }) => run_chat(
            prompt,
            headless,
            no_tui,
            local_endpoint,
            provider,
            model,
            region,
            endpoint,
        ),
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
                run_chat(None, false, false, None, None, None, None, None)
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

#[allow(clippy::too_many_arguments)]
fn run_chat(
    prompt: Option<String>,
    headless: bool,
    no_tui: bool,
    local_endpoint: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    region: Option<String>,
    endpoint: Option<String>,
) -> Result<(), String> {
    let provider_result =
        resolve_provider_config(local_endpoint, provider, model, region, endpoint);

    // For headless/plaintext modes, a provider is required upfront.
    // For interactive TUI, we allow starting without a provider and
    // show the error as a chat message so the user can fix it.
    if headless {
        let provider_cfg = provider_result?;
        let prompt = prompt.ok_or_else(|| {
            "Prompt required in headless mode. Use: aegis chat --headless -p 'your prompt'"
                .to_string()
        })?;

        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
        return rt.block_on(async { run_headless_chat(&prompt, &provider_cfg).await });
    }

    // Check for plain-text mode: explicit flag or env detection
    let use_plain_text = aegis_tui::terminal::should_use_plain_text(no_tui);
    if use_plain_text {
        let provider_cfg = provider_result?;
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
        return rt.block_on(async { run_plaintext_chat(&provider_cfg, prompt).await });
    }

    // Interactive TUI mode -- start even without a provider.
    // If provider resolution failed, pass the error message to the TUI
    // so it can display it as a chat message.
    let (provider_cfg, startup_error) = match provider_result {
        Ok(cfg) => (cfg, None),
        Err(e) => {
            // Use a dummy local config so the TUI can start.
            // Agent requests will fail, but the user can see the error and fix it.
            (
                aegis_llm::config::ProviderConfig::local("http://localhost:11434/v1", "llama3"),
                Some(e),
            )
        }
    };

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
    rt.block_on(async { run_interactive_chat(&provider_cfg, prompt, startup_error).await })
}

fn resolve_provider_config(
    local_endpoint: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    region: Option<String>,
    endpoint: Option<String>,
) -> Result<aegis_llm::config::ProviderConfig, String> {
    use aegis_llm::config::{ProviderConfig, ProviderKind};

    // CLI --local-endpoint shortcut (backwards compatible)
    if let Some(ep) = local_endpoint {
        return Ok(ProviderConfig::local(&ep, "default"));
    }

    // CLI --provider flag overrides config file
    if let Some(ref p) = provider {
        let kind = match p.to_lowercase().as_str() {
            "local" => ProviderKind::Local,
            "vertex" => ProviderKind::Vertex,
            "bedrock" => ProviderKind::Bedrock,
            "azure" => ProviderKind::Azure,
            other => {
                return Err(format!(
                    "Unknown provider '{}'. Options: local, vertex, bedrock, azure",
                    other
                ));
            }
        };
        let model = model.unwrap_or_else(|| match kind {
            ProviderKind::Local => "llama3".to_string(),
            ProviderKind::Vertex => "gemini-2.5-pro-001".to_string(),
            ProviderKind::Bedrock => "us.anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
            ProviderKind::Azure => "gpt-4o".to_string(),
        });
        let endpoint = endpoint.unwrap_or_else(|| match kind {
            ProviderKind::Local => "http://localhost:11434/v1".to_string(),
            ProviderKind::Vertex => "https://us-central1-aiplatform.googleapis.com".to_string(),
            ProviderKind::Bedrock => format!(
                "https://bedrock-runtime.{}.amazonaws.com",
                region.as_deref().unwrap_or("us-east-1")
            ),
            ProviderKind::Azure => String::new(),
        });
        if kind == ProviderKind::Azure && endpoint.is_empty() {
            return Err(
                "Azure provider requires --endpoint (e.g.,                  https://myresource.openai.azure.com)"
                    .to_string(),
            );
        }
        return Ok(ProviderConfig {
            kind,
            model,
            endpoint,
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: region.clone(),
        });
    }

    // Try loading from saved config first.
    let config_result = aegis_onboard::config::AegisConfig::default_path()
        .ok()
        .and_then(|path| aegis_onboard::config::AegisConfig::load(&path).ok());

    if let Some(config) = config_result {
        let kind = match config.backend.provider.as_str() {
            "vertex" => ProviderKind::Vertex,
            "bedrock" => ProviderKind::Bedrock,
            "azure" => ProviderKind::Azure,
            _ => ProviderKind::Local,
        };

        // For Vertex, extract project_id from infra outputs if not explicitly set
        let project_id = config
            .infra
            .plugins
            .get("gcp-assured-workloads")
            .and_then(|p| p.outputs.get("project_id"))
            .cloned();

        let region = config.backend.region.clone();

        let cfg = ProviderConfig {
            kind,
            model: config.backend.model,
            endpoint: config.backend.endpoint,
            max_tokens: config.backend.max_tokens,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id,
            region,
        };

        // Probe the configured endpoint to verify it's reachable before
        // accepting it. If unreachable, fall through to auto-discovery.
        let endpoint = cfg.endpoint.clone();
        let reachable = {
            let rt = tokio::runtime::Handle::try_current();
            match rt {
                Ok(handle) => std::thread::scope(|s| {
                    s.spawn(|| handle.block_on(aegis_llm::discovery::probe_endpoint(&endpoint)))
                        .join()
                        .unwrap_or(false)
                }),
                Err(_) => {
                    let tmp = tokio::runtime::Runtime::new().ok();
                    tmp.map(|rt| rt.block_on(aegis_llm::discovery::probe_endpoint(&endpoint)))
                        .unwrap_or(false)
                }
            }
        };

        if reachable {
            return Ok(cfg);
        }
        tracing::warn!(
            endpoint = %endpoint,
            "configured provider unreachable, falling back to discovery"
        );
    }

    // No config or configured provider failed -- run auto-discovery.
    let rt = tokio::runtime::Handle::try_current();
    let discovered = match rt {
        Ok(handle) => {
            // Already inside an async runtime; spawn a blocking task to
            // run discovery without nesting runtimes.
            std::thread::scope(|s| {
                s.spawn(|| handle.block_on(aegis_llm::discovery::discover_provider()))
                    .join()
                    .unwrap()
            })
        }
        Err(_) => {
            // No runtime yet; create a temporary one for discovery.
            let tmp_rt =
                tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
            tmp_rt.block_on(aegis_llm::discovery::discover_provider())
        }
    };

    if let Ok(dp) = discovered {
        eprintln!("aegis: auto-discovered provider: {}", dp.name);
        return Ok(dp.config);
    }

    // Discovery failed -- try to auto-start Ollama if it's installed.
    let has_ollama = std::process::Command::new("which")
        .arg("ollama")
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_ollama {
        return Err("No LLM backend found and Ollama is not installed.\n  \
             Local:  brew install ollama  (macOS)\n          \
             curl -fsSL https://ollama.com/install.sh | sh  (Linux)\n  \
             Cloud:  aegis init (configure Vertex AI / Bedrock)"
            .to_string());
    }

    eprintln!("aegis: starting ollama serve...");
    let _ = std::process::Command::new("ollama")
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    // Wait up to 5 seconds for Ollama to come up
    let ollama_endpoint = "http://localhost:11434/v1";
    let mut started = false;
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let probe = match tokio::runtime::Handle::try_current() {
            Ok(handle) => std::thread::scope(|s| {
                s.spawn(|| handle.block_on(aegis_llm::discovery::probe_endpoint(ollama_endpoint)))
                    .join()
                    .unwrap_or(false)
            }),
            Err(_) => {
                if let Ok(rt) = tokio::runtime::Runtime::new() {
                    rt.block_on(aegis_llm::discovery::probe_endpoint(ollama_endpoint))
                } else {
                    false
                }
            }
        };
        if probe {
            started = true;
            break;
        }
    }

    if !started {
        // Ollama is installed but failed to start. Try pulling the model
        // anyway -- `ollama pull` starts the server implicitly on some
        // platforms.
        eprintln!("aegis: ollama serve did not respond, pulling llama3...");
        let pull = std::process::Command::new("ollama")
            .args(["pull", "llama3"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();
        match pull {
            Ok(o) if o.status.success() => {}
            _ => {
                return Err("Ollama is installed but failed to start.\n  \
                     Try manually: ollama serve\n  \
                     Then: aegis chat"
                    .to_string());
            }
        }
    }

    // Re-run discovery now that Ollama is running
    let rediscovered = match tokio::runtime::Handle::try_current() {
        Ok(handle) => std::thread::scope(|s| {
            s.spawn(|| handle.block_on(aegis_llm::discovery::discover_provider()))
                .join()
                .unwrap()
        }),
        Err(_) => {
            let tmp_rt =
                tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
            tmp_rt.block_on(aegis_llm::discovery::discover_provider())
        }
    };

    match rediscovered {
        Ok(dp) => {
            eprintln!("aegis: auto-started Ollama, using {}", dp.name);
            Ok(dp.config)
        }
        Err(_) => {
            // Ollama is running but has no models -- pull llama3
            eprintln!("aegis: pulling llama3...");
            let pull = std::process::Command::new("ollama")
                .args(["pull", "llama3"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output();
            match pull {
                Ok(o) if o.status.success() => {
                    eprintln!("aegis: llama3 ready");
                    Ok(aegis_llm::config::ProviderConfig::local(
                        "http://localhost:11434/v1",
                        "llama3",
                    ))
                }
                _ => Err("Ollama started but failed to pull llama3.\n  \
                     Try manually: ollama pull llama3\n  \
                     Then: aegis chat"
                    .to_string()),
            }
        }
    }
}

async fn run_headless_chat(
    prompt: &str,
    provider_config: &aegis_llm::config::ProviderConfig,
) -> Result<(), String> {
    // 1. Create LLM provider via factory
    let provider =
        aegis_llm::provider::create_provider(provider_config).map_err(|e| e.to_string())?;

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
        system_prompt: build_system_prompt(),
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

/// Format a ToolCall enum variant for plain-text display.
fn format_tool_call_plain(call: &ToolCall) -> String {
    match call {
        ToolCall::ReadFile { path } => format!("read_file: {path}"),
        ToolCall::WriteFile { path, .. } => format!("write_file: {path}"),
        ToolCall::RunCommand { command, .. } => format!("run_command: {command}"),
        ToolCall::ListDir { path } => format!("list_dir: {path}"),
        ToolCall::Grep { pattern, path } => {
            format!("grep: {pattern} in {path}")
        }
        ToolCall::McpTool {
            qualified_name,
            arguments,
        } => format!("mcp: {qualified_name}({arguments})"),
    }
}

/// Plain-text interactive chat loop (REQ-TUI-013).
///
/// No ratatui, no alternate screen, no raw mode. Reads lines from stdin,
/// sends them through the agent, and prints responses to stdout with
/// simple text prefixes. Compatible with screen readers and dumb terminals.
async fn run_plaintext_chat(
    provider_config: &aegis_llm::config::ProviderConfig,
    initial_prompt: Option<String>,
) -> Result<(), String> {
    use std::io::{BufRead, Write};

    let model = &provider_config.model;
    println!("[system] aegis plain-text mode (model: {model})");
    println!("[system] type /help for commands, /quit to exit");
    println!();

    // Process an initial prompt if supplied
    if let Some(ref prompt) = initial_prompt {
        println!("> {prompt}");
        run_plaintext_turn(prompt, provider_config).await?;
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    loop {
        // Prompt indicator
        print!("aegis> ");
        stdout.flush().map_err(|e| format!("IO error: {e}"))?;

        let mut line = String::new();
        let bytes = stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| format!("IO error: {e}"))?;

        // EOF (Ctrl-D)
        if bytes == 0 {
            println!();
            println!("[system] goodbye");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Slash commands
        match trimmed {
            "/quit" | "/exit" => {
                println!("[system] goodbye");
                break;
            }
            "/help" => {
                println!("[system] commands:");
                println!("[system]   /help  -- show this help");
                println!("[system]   /quit  -- exit the session");
                println!();
                continue;
            }
            _ => {}
        }

        // Echo user input
        println!("> {trimmed}");

        // Run the agent for this turn
        run_plaintext_turn(trimmed, provider_config).await?;
    }

    Ok(())
}

/// Execute a single agent turn in plain-text mode, printing streamed output.
async fn run_plaintext_turn(
    prompt: &str,
    provider_config: &aegis_llm::config::ProviderConfig,
) -> Result<(), String> {
    use aegis_domain::ports::StreamEvent;

    let provider =
        aegis_llm::provider::create_provider(provider_config).map_err(|e| e.to_string())?;

    let filter: Arc<aegis_security::aegisignore::AegisIgnore> =
        Arc::new(aegis_security::aegisignore::AegisIgnore::with_defaults());
    let work_dir = std::env::current_dir().map_err(|e| format!("Cannot get cwd: {e}"))?;
    let executor = aegis_agent::tools::BuiltinExecutor::new(filter.clone(), &work_dir);

    let gate = HeadlessApprovalGate;

    let log_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".aegis/logs");
    let ledger = aegis_audit::ledger::JsonlLedger::new(&log_dir)
        .await
        .map_err(|e| e.to_string())?;

    let config = aegis_agent::loop_runner::AgentConfig {
        max_iterations: 20,
        system_prompt: build_system_prompt(),
    };

    // Set up stream event channel for tool-use visibility
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();

    // Spawn a task to print stream events as they arrive
    let printer = tokio::spawn(async move {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        while let Some(event) = stream_rx.recv().await {
            match event {
                StreamEvent::Token(text) => {
                    print!("{text}");
                    let _ = stdout.flush();
                }
                StreamEvent::ToolUse(call) => {
                    println!();
                    let desc = format_tool_call_plain(&call);
                    println!("[tool] {desc}");
                }
                StreamEvent::Done {
                    input_tokens,
                    output_tokens,
                } => {
                    println!();
                    eprintln!("[{input_tokens}in + {output_tokens}out tokens]");
                }
                StreamEvent::Error(msg) => {
                    println!();
                    println!("[error] {msg}");
                }
                StreamEvent::RetryableError { message, .. } => {
                    println!();
                    println!("[error] {message}");
                }
            }
        }
    });

    let agent = aegis_agent::loop_runner::AgentLoop::new(
        provider, gate, executor, ledger, filter, config,
    )
    .with_event_sink(stream_tx);

    match agent.run(prompt).await {
        Ok(_result) => {}
        Err(e) => {
            println!("[error] {e}");
        }
    }

    // Wait for the printer to finish draining
    let _ = printer.await;
    println!();

    Ok(())
}

/// Retry a fallible I/O operation on EINTR (interrupted system call).
/// Terminal setup syscalls (ioctl, write) can be interrupted by signals
/// during job-control transitions (bg/fg in dev-run.sh). Retrying is
/// safe because these calls are idempotent.
fn retry_eintr<F, T>(mut f: F) -> std::io::Result<T>
where
    F: FnMut() -> std::io::Result<T>,
{
    loop {
        match f() {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

async fn run_interactive_chat(
    provider_config: &aegis_llm::config::ProviderConfig,
    initial_prompt: Option<String>,
    startup_error: Option<String>,
) -> Result<(), String> {
    use crossterm::event as ct_event;
    use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
    use ratatui::Terminal;
    use ratatui::backend::CrosstermBackend;
    use tokio::sync::mpsc;

    // 0. Install a panic hook that restores terminal state. Without this,
    // a panic leaves the terminal in raw mode with alternate screen active,
    // making the shell unusable until `reset` is run manually.
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::terminal::LeaveAlternateScreen
        );
        default_panic(info);
    }));

    // 1. Set up terminal (retry on EINTR from job-control signals)
    //
    // Ignore SIGTTOU so that tcsetattr / write succeed even if we are
    // briefly in a background process group (e.g. dev-run.sh bg/fg dance).
    // Restored after terminal setup is complete.
    #[cfg(unix)]
    let prev_sigttou = unsafe { libc::signal(libc::SIGTTOU, libc::SIG_IGN) };

    retry_eintr(crossterm::terminal::enable_raw_mode).map_err(|e| {
        tracing::error!(%e, "enable_raw_mode failed");
        format!("Terminal error: {e}")
    })?;
    let mut stdout = std::io::stdout();
    retry_eintr(|| {
        crossterm::execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableMouseCapture,
            aegis_tui::terminal::CURSOR_STYLE
        )
    })
    .map_err(|e| {
        tracing::error!(%e, "terminal setup failed");
        format!("Terminal error: {e}")
    })?;

    // Enable Kitty keyboard protocol if the terminal supports it.
    // This allows Shift+Enter to be distinguished from Enter.
    let enhanced_keyboard = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced_keyboard {
        retry_eintr(|| {
            crossterm::execute!(
                stdout,
                crossterm::event::PushKeyboardEnhancementFlags(
                    crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                )
            )
        })
        .map_err(|e| {
            tracing::error!(%e, "keyboard enhancement failed");
            format!("Keyboard enhancement error: {e}")
        })?;
    }

    let mut terminal = retry_eintr(|| Terminal::new(CrosstermBackend::new(std::io::stdout())))
        .map_err(|e| {
            tracing::error!(%e, "Terminal::new failed");
            format!("Terminal error: {e}")
        })?;

    // Restore previous SIGTTOU disposition now that terminal setup is done.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGTTOU, prev_sigttou);
    }

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
    let agent_provider_config = provider_config.clone();
    let event_tx_agent = event_tx.clone();
    tokio::spawn(async move {
        while let Some(prompt) = agent_input_rx.recv().await {
            let result = run_agent_for_tui(
                &prompt,
                &agent_provider_config,
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
    let mut app = App::new(&provider_config.model);
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

    // 10. Show startup error if provider resolution failed
    if let Some(err) = startup_error {
        app.messages
            .push(aegis_tui::messages::ChatMessage::error(err));
        app.messages.push(aegis_tui::messages::ChatMessage::system(
            "No LLM backend connected.\n\n\
                 To get started:\n\
                 \n\
                   /connect http://localhost:11434/v1    Connect to Ollama\n\
                   /connect http://localhost:8080/v1     Connect to vLLM/TGI\n\
                   /doctor                               Check connectivity\n\
                 \n\
                 Or start a local model server:\n\
                   ollama serve && ollama pull llama3"
                .to_string(),
        ));
    }

    // 11. If initial prompt provided, submit it immediately
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
                        spinner_frame: (app.tick_count % 4) as u8,
                        command_palette: app.command_palette.view(),
                        ghost_text: app.input.ghost_text.clone(),
                        waiting_text: app.waiting_text(),
                        file_picker: app.file_picker.as_ref().map(|fp| {
                            aegis_tui::layout::FilePickerView {
                                query: fp.query.clone(),
                                entries: fp.tree_entries(),
                                selected: fp.selected,
                                preview: fp.preview_content(),
                                preview_extension: fp.selected_extension(),
                            }
                        }),
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
    provider_config: &aegis_llm::config::ProviderConfig,
    gate: aegis_hitl::approval::ChannelApprovalGate,
    event_tx: &tokio::sync::mpsc::UnboundedSender<TuiEvent>,
) -> Result<(), String> {
    let provider =
        aegis_llm::provider::create_provider(provider_config).map_err(|e| e.to_string())?;

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
        system_prompt: build_system_prompt(),
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
                StreamEvent::RetryableError { message, .. } => TuiEvent::AgentError(message),
            };
            if event_tx_stream.send(tui_event).is_err() {
                break;
            }
        }
    });

    let mut agent = aegis_agent::loop_runner::AgentLoop::new(
        provider, gate, executor, ledger, filter, config,
    )
    .with_event_sink(stream_tx);

    // REQ-AGENT-022: Connect MCP servers from config (if any).
    let aegis_config = aegis_onboard::config::AegisConfig::default_path()
        .ok()
        .and_then(|p| aegis_onboard::config::AegisConfig::load(&p).ok());
    if let Some(aegis_config) = aegis_config
        && !aegis_config.mcp_servers.is_empty()
    {
        let mut mcp_mgr = aegis_agent::mcp::McpManager::new();
        for srv in &aegis_config.mcp_servers {
            match mcp_mgr.connect(srv.clone()).await {
                Ok(tools) => {
                    tracing::info!(
                        server = %srv.name,
                        tools = tools.len(),
                        "MCP server connected"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        server = %srv.name,
                        %e,
                        "MCP server connection failed"
                    );
                }
            }
        }
        agent = agent.with_mcp_manager(mcp_mgr);
    }

    agent.run(prompt).await.map_err(|e| e.to_string())?;
    Ok(())
}
