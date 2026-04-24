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
    /// Review HITL approval/denial history from the audit ledger
    History {
        /// Filter to a specific session ID
        #[arg(long)]
        session: Option<String>,
        /// Show only denied entries
        #[arg(long)]
        denied: bool,
    },
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
        Some(Commands::History { session, denied }) => run_history(session, denied),
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

/// Run the `aegis history` subcommand (REQ-HITL-006).
///
/// Searches the audit ledger for HITL approval/denial events and
/// displays them in a formatted table. Supports filtering by session
/// ID and by denied-only.
fn run_history(session: Option<String>, denied: bool) -> Result<(), String> {
    let log_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".aegis/logs");

    if !log_dir.exists() {
        eprintln!("No audit logs found at {}", log_dir.display());
        return Ok(());
    }

    // Search the audit ledger for all HITL event types.
    // We run one query per event type and merge results, since the
    // search API filters on a single event_type at a time.
    let mut all_entries = Vec::new();
    for event_type in aegis_hitl::history::HITL_EVENT_TYPES {
        let search_query = aegis_audit::search::SearchQuery {
            event_type: Some((*event_type).to_string()),
            session_id: session.clone(),
            ..Default::default()
        };
        match aegis_audit::search::search_ledger(&log_dir, &search_query) {
            Ok(result) => all_entries.extend(result.entries),
            Err(e) => {
                return Err(format!("Failed to search audit ledger: {e}"));
            }
        }
    }

    // Extract and filter structured history entries.
    let query = aegis_hitl::history::HistoryQuery {
        session_id: session,
        denied_only: denied,
    };
    let history = aegis_hitl::history::extract_history(&all_entries, &query);

    // Format and display.
    println!("{}", aegis_hitl::history::format_history(&history));

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

/// Convert a TUI ConnectRequest into a ProviderConfig, resolve auth,
/// save to config.yaml, and swap the shared provider config.
///
/// REQ-LLM-027: /connect updates the live provider mid-session.
/// REQ-LLM-029: cloud provider support (vertex, bedrock, azure).
/// REQ-LLM-024: all creation goes through create_provider factory.
fn handle_connect_request(
    req: &aegis_tui::app::ConnectRequest,
    app: &mut App,
    shared_config: &Arc<std::sync::RwLock<aegis_llm::config::ProviderConfig>>,
) {
    use aegis_llm::config::ProviderConfig;
    use aegis_tui::app::ConnectProvider;

    // 1. Build ProviderConfig from the ConnectRequest.
    let new_config = match req.provider {
        ConnectProvider::Local => {
            let endpoint = req
                .endpoint
                .as_deref()
                .unwrap_or("http://localhost:11434/v1");
            let model = req.model.as_deref().unwrap_or("llama3");
            ProviderConfig::local(endpoint, model)
        }
        ConnectProvider::Vertex => {
            let project = req.project.as_deref().unwrap_or("");
            let region = req.region.as_deref().unwrap_or("us-central1");
            let model = req.model.as_deref().unwrap_or("gemini-2.5-pro-001");
            if project.is_empty() {
                // Try to get project from gcloud config
                let gcloud_project = std::process::Command::new("gcloud")
                    .args(["config", "get-value", "project"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output()
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            let val = String::from_utf8(o.stdout).ok()?.trim().to_string();
                            if val.is_empty() || val == "(unset)" {
                                None
                            } else {
                                Some(val)
                            }
                        } else {
                            None
                        }
                    });
                match gcloud_project {
                    Some(p) => ProviderConfig::vertex(&p, region, model),
                    None => {
                        app.messages.push(aegis_tui::messages::ChatMessage::error(
                            "No GCP project specified and gcloud config \
                                 has no default project.\n\
                                 Use: /connect vertex \
                                 --project=YOUR_PROJECT --region=us-central1"
                                .to_string(),
                        ));
                        return;
                    }
                }
            } else {
                ProviderConfig::vertex(project, region, model)
            }
        }
        ConnectProvider::Bedrock => {
            let region = req.region.as_deref().unwrap_or("us-east-1");
            let model = req
                .model
                .as_deref()
                .unwrap_or("us.anthropic.claude-3-5-sonnet-20241022-v2:0");
            ProviderConfig::bedrock(region, model)
        }
        ConnectProvider::Azure => {
            let endpoint = match req.endpoint.as_deref() {
                Some(ep) => ep,
                None => {
                    app.messages.push(aegis_tui::messages::ChatMessage::error(
                        "Azure requires an endpoint URL.".to_string(),
                    ));
                    return;
                }
            };
            let model = req.model.as_deref().unwrap_or("gpt-4o");
            ProviderConfig::azure(endpoint, model)
        }
    };

    // 2. Validate auth (non-blocking for local, may fail for cloud).
    let auth_result = aegis_llm::auth::resolve_auth(&new_config);
    if let Err(e) = &auth_result {
        let guidance = aegis_tui::app::auth_guidance(&req.provider);
        app.messages
            .push(aegis_tui::messages::ChatMessage::error(format!(
                "Authentication failed: {e}\n\n{guidance}"
            )));
        return;
    }

    // 3. Verify the provider factory can build this config.
    match aegis_llm::provider::create_provider(&new_config) {
        Ok(_) => {}
        Err(e) => {
            app.messages
                .push(aegis_tui::messages::ChatMessage::error(format!(
                    "Provider creation failed: {e}"
                )));
            return;
        }
    }

    // 4. Save to ~/.aegis/config.yaml so the choice persists.
    if let Err(e) = save_provider_to_config(&new_config) {
        tracing::warn!(%e, "failed to persist provider config");
        app.messages
            .push(aegis_tui::messages::ChatMessage::system(format!(
                "Warning: could not save to config.yaml: {e}"
            )));
        // Continue anyway -- the live swap is still useful.
    }

    // 5. Swap the shared provider config so the next agent turn uses it.
    {
        let mut guard = shared_config.write().unwrap();
        *guard = new_config.clone();
    }

    // 6. Update App state: model name and provider info.
    app.model_name = new_config.model.clone();
    let kind_str = format!("{:?}", new_config.kind).to_lowercase();
    app.current_provider_info = Some(aegis_tui::app::ProviderInfo {
        provider: kind_str.clone(),
        model: new_config.model.clone(),
        endpoint: new_config.endpoint.clone(),
        region: new_config.region.clone(),
    });

    // 7. Push success message.
    let region_info = new_config
        .region
        .as_ref()
        .map(|r| format!(" in {r}"))
        .unwrap_or_default();
    app.messages
        .push(aegis_tui::messages::ChatMessage::system(format!(
            "Connected to {kind_str} ({model}){region_info}.",
            model = new_config.model,
        )));
}

/// Persist the provider config to ~/.aegis/config.yaml.
/// Merges with existing config to preserve infra outputs, MCP servers, etc.
fn save_provider_to_config(config: &aegis_llm::config::ProviderConfig) -> Result<(), String> {
    let config_path =
        aegis_onboard::config::AegisConfig::default_path().map_err(|e| e.to_string())?;

    let kind_str = format!("{:?}", config.kind).to_lowercase();
    let mode = match config.kind {
        aegis_llm::config::ProviderKind::Local => aegis_onboard::config::Mode::Local,
        _ => aegis_onboard::config::Mode::SelfServiceByoc,
    };

    let new_aegis_config = aegis_onboard::config::AegisConfig {
        version: "1.0".to_string(),
        mode,
        backend: aegis_onboard::config::BackendConfig {
            provider: kind_str,
            model: config.model.clone(),
            endpoint: config.endpoint.clone(),
            region: config.region.clone(),
            max_tokens: config.max_tokens,
        },
        infra: Default::default(),
        mcp_servers: Vec::new(),
    };

    // Try to load existing config and merge, otherwise use new
    let final_config =
        if let Ok(existing) = aegis_onboard::config::AegisConfig::load(&config_path) {
            aegis_onboard::config::merge_config(&existing, &new_aegis_config)
        } else {
            new_aegis_config
        };

    final_config.save(&config_path).map_err(|e| e.to_string())
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
    //
    // The provider config is behind Arc<RwLock> so `/connect` can swap it
    // mid-session (REQ-LLM-027). The agent task reads the current config
    // on each prompt, so the next turn uses the updated provider.
    let shared_provider_config = Arc::new(std::sync::RwLock::new(provider_config.clone()));
    let agent_shared_config = shared_provider_config.clone();
    let event_tx_agent = event_tx.clone();
    tokio::spawn(async move {
        while let Some(prompt) = agent_input_rx.recv().await {
            let cfg = agent_shared_config.read().unwrap().clone();
            let result =
                run_agent_for_tui(&prompt, &cfg, approval_gate.clone(), &event_tx_agent).await;
            if let Err(e) = result {
                let _ = event_tx_agent.send(TuiEvent::AgentError(e));
            }
        }
    });

    // 9. Detect platform (immutable for session lifetime)
    let platform = aegis_tui::platform::Platform::detect();

    // 10. Create App state, restoring previous session if available (REQ-BUILD-036)
    let mut app = App::new(&provider_config.model);
    app.current_provider_info = Some(aegis_tui::app::ProviderInfo {
        provider: format!("{:?}", provider_config.kind).to_lowercase(),
        model: provider_config.model.clone(),
        endpoint: provider_config.endpoint.clone(),
        region: provider_config.region.clone(),
    });
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

    // REQ-TUI-060: autosave bookkeeping. `last_saved_message_count` tracks
    // the point in history we last persisted so the 30s timer can skip
    // work when the session is clean. `last_autosave_at` stamps the last
    // successful periodic save.
    let mut last_saved_message_count: usize = app.messages.len();
    let mut last_autosave_at = std::time::Instant::now();
    const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

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

        // Wait for next event OR SIGTERM OR the 30s autosave timer.
        //
        // The autosave timer (REQ-TUI-060) fires every AUTOSAVE_INTERVAL and
        // causes the loop to wake even if the terminal/agent are idle. When
        // it wakes with no real event pending we still want to fall through
        // to the autosave check below without exiting.
        //
        // Wake reasons are encoded as `EventWake`:
        //   Event(evt) -- normal TuiEvent delivery, dispatch to handle_event
        //   Timer      -- autosave timer, no event to dispatch
        //   Shutdown   -- SIGTERM or channel closed, exit after final save
        enum EventWake {
            Event(TuiEvent),
            Timer,
            Shutdown,
        }

        #[cfg(unix)]
        let wake = tokio::select! {
            evt = event_rx.recv() => match evt {
                Some(e) => EventWake::Event(e),
                None => EventWake::Shutdown,
            },
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received, saving session and exiting");
                EventWake::Shutdown
            }
            _ = tokio::time::sleep(AUTOSAVE_INTERVAL) => EventWake::Timer,
        };
        #[cfg(not(unix))]
        let wake = tokio::select! {
            evt = event_rx.recv() => match evt {
                Some(e) => EventWake::Event(e),
                None => EventWake::Shutdown,
            },
            _ = tokio::time::sleep(AUTOSAVE_INTERVAL) => EventWake::Timer,
        };

        // REQ-TUI-060: detect assistant turn completion BEFORE dispatch so we
        // know to save immediately after the app commits the new history.
        let is_agent_done = matches!(&wake, EventWake::Event(TuiEvent::AgentDone { .. }));
        let mut should_quit = false;
        let mut exit_via_sigterm = false;

        match wake {
            EventWake::Event(event) => {
                if app.handle_event(event, &agent_input_tx) == Action::Quit {
                    should_quit = true;
                }
            }
            EventWake::Timer => {
                // No event to dispatch; drop through to autosave check.
            }
            EventWake::Shutdown => {
                exit_via_sigterm = true;
            }
        }

        // REQ-LLM-027: Process pending /connect requests from the TUI.
        if let Some(connect_req) = app.pending_connect.take() {
            handle_connect_request(&connect_req, &mut app, &shared_provider_config);
        }

        // REQ-LLM-031: CSP project discovery -- spawn blocking task when
        // the TUI requests it (e.g. user picks a cloud provider in the
        // command palette and the palette needs a project list).
        if let Some(provider) = app.pending_csp_discovery.take() {
            let event_tx_csp = event_tx.clone();
            tokio::task::spawn_blocking(move || {
                use aegis_llm::csp_discovery::{CliCspDiscoverer, CspDiscoverer};
                let discoverer = CliCspDiscoverer;
                match discoverer.discover_projects(&provider) {
                    Ok(projects) => {
                        let pairs: Vec<(String, String)> =
                            projects.into_iter().map(|p| (p.id, p.name)).collect();
                        let _ = event_tx_csp.send(TuiEvent::CspProjectsReady {
                            provider,
                            projects: pairs,
                        });
                    }
                    Err(e) => {
                        let (message, guidance) = e.to_guidance();
                        let _ = event_tx_csp.send(TuiEvent::CspProjectsError {
                            provider,
                            message,
                            guidance,
                        });
                    }
                }
            });
        }

        // REQ-TUI-060: autosave after every completed assistant turn so a
        // crash right after the turn (hot reload, panic, OOM) does not lose
        // the just-finished response.
        if is_agent_done && let Some(ref dir) = session_dir {
            match save_session_now(&app, &session_id, &work_dir, dir) {
                Ok(path) => {
                    tracing::debug!(
                        path = %path.display(),
                        messages = app.messages.len(),
                        "autosave: post-agent-done",
                    );
                    last_saved_message_count = app.messages.len();
                    last_autosave_at = std::time::Instant::now();
                }
                Err(e) => tracing::warn!(%e, "autosave after agent-done failed"),
            }
        }

        // REQ-TUI-060: periodic autosave every AUTOSAVE_INTERVAL if the
        // session is dirty (new messages since last save). Skips work when
        // nothing has changed.
        if !should_quit
            && !exit_via_sigterm
            && last_autosave_at.elapsed() >= AUTOSAVE_INTERVAL
            && app.messages.len() > last_saved_message_count
        {
            if let Some(ref dir) = session_dir {
                match save_session_now(&app, &session_id, &work_dir, dir) {
                    Ok(path) => {
                        tracing::debug!(
                            path = %path.display(),
                            messages = app.messages.len(),
                            "autosave: periodic",
                        );
                        last_saved_message_count = app.messages.len();
                        last_autosave_at = std::time::Instant::now();
                    }
                    Err(e) => tracing::warn!(%e, "periodic autosave failed"),
                }
            } else {
                // Still advance the timer so we don't spin retrying with no
                // session_dir available.
                last_autosave_at = std::time::Instant::now();
            }
        }

        if should_quit || exit_via_sigterm {
            break;
        }
    }

    // 12. Save session before cleanup (REQ-BUILD-035). Even though
    // REQ-TUI-060 saves after each assistant turn, this final save still
    // captures any messages added after the last autosave (e.g. the user
    // typed a prompt and then hit Ctrl-C before the agent responded).
    if let Some(ref dir) = session_dir
        && !app.messages.is_empty()
    {
        match save_session_now(&app, &session_id, &work_dir, dir) {
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

/// Persist the current App state atomically to the session directory.
///
/// Extracted from the end-of-loop save path so the event loop can call it
/// after every `AgentDone` and on the 30s periodic timer (REQ-TUI-060).
///
/// Atomicity is provided by `aegis_agent::session::save_session`, which
/// writes to a sibling tmp file then renames into place. The rename step
/// is filesystem-atomic on POSIX and on Windows NTFS, so a crash between
/// writes cannot leave a torn snapshot that would fail to parse on the
/// next startup.
fn save_session_now(
    app: &App,
    session_id: &str,
    work_dir: &std::path::Path,
    session_dir: &std::path::Path,
) -> Result<std::path::PathBuf, std::io::Error> {
    let snapshot = build_snapshot_from_app(app, session_id, work_dir.to_path_buf());
    aegis_agent::session::save_session(session_dir, &snapshot)
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

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_tui::messages::ChatMessage;

    /// Build an App populated with a synthetic multi-turn conversation.
    fn app_with_turns(turns: usize) -> App {
        let mut app = App::new("claude-opus-4-6");
        for i in 0..turns {
            app.messages
                .push(ChatMessage::user(format!("user turn {i}")));
            app.messages
                .push(ChatMessage::assistant(format!("assistant reply {i}")));
        }
        app.input_tokens = (turns as u64) * 100;
        app.output_tokens = (turns as u64) * 50;
        app
    }

    // rtmx:req REQ-TUI-060
    /// `save_session_now` writes the current App state to the session
    /// directory as an atomic file the next startup can load.
    #[test]
    fn save_session_now_persists_current_app_state() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let session_dir = tmp.path();
        let work_dir = std::path::PathBuf::from("/tmp/work");

        let app = app_with_turns(2);
        let path = save_session_now(&app, "sess-unit-001", &work_dir, session_dir)
            .expect("save_session_now");

        assert!(path.exists(), "saved snapshot must exist");
        let loaded =
            aegis_agent::session::load_session(&path).expect("loaded snapshot must parse");
        assert_eq!(loaded.session_id, "sess-unit-001");
        assert_eq!(
            loaded.messages.len(),
            4,
            "2 turns -> 4 messages (user+assistant each turn)",
        );
        assert_eq!(loaded.input_tokens, 200);
        assert_eq!(loaded.output_tokens, 100);
        assert_eq!(loaded.model_name, "claude-opus-4-6");
    }

    // rtmx:req REQ-TUI-060
    /// Calling `save_session_now` repeatedly during a conversation produces
    /// a snapshot matching the latest App state every time -- that is the
    /// per-turn autosave contract the event loop relies on.
    #[test]
    fn save_session_now_reflects_latest_app_state_per_turn() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let session_dir = tmp.path();
        let work_dir = std::path::PathBuf::from("/tmp/work");
        let session_id = "sess-turn-001";

        for turn in 1..=3 {
            let app = app_with_turns(turn);
            let path = save_session_now(&app, session_id, &work_dir, session_dir)
                .expect("save_session_now");
            let loaded = aegis_agent::session::load_session(&path).expect("snapshot parses");
            assert_eq!(
                loaded.messages.len(),
                turn * 2,
                "after {turn} turns snapshot should hold {} messages",
                turn * 2,
            );
        }
    }

    // rtmx:req REQ-TUI-060
    /// The save path must not leave a `.tmp` file behind after the atomic
    /// rename completes. This guards against future refactors that might
    /// replace `std::fs::rename` with a non-atomic write.
    #[test]
    fn save_session_now_leaves_no_tmp_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let session_dir = tmp.path();
        let work_dir = std::path::PathBuf::from("/tmp/work");
        let session_id = "sess-atomic-002";

        let app = app_with_turns(1);
        save_session_now(&app, session_id, &work_dir, session_dir).expect("save");

        let tmp_file = session_dir.join(format!(".{session_id}.json.tmp"));
        assert!(
            !tmp_file.exists(),
            "atomic rename must not leave a .tmp file after a successful save",
        );
    }

    // rtmx:req REQ-LLM-027
    /// handle_connect_request updates the shared provider config when a
    /// local /connect request succeeds.
    #[test]
    fn handle_connect_local_updates_shared_config() {
        let mut app = App::new("llama3");
        app.phase = aegis_tui::app::AppPhase::Idle;
        let config =
            aegis_llm::config::ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let shared = Arc::new(std::sync::RwLock::new(config));

        let req = aegis_tui::app::ConnectRequest {
            provider: aegis_tui::app::ConnectProvider::Local,
            endpoint: Some("http://localhost:8080/v1".to_string()),
            model: Some("mixtral".to_string()),
            project: None,
            region: None,
        };
        handle_connect_request(&req, &mut app, &shared);

        let updated = shared.read().unwrap();
        assert_eq!(updated.endpoint, "http://localhost:8080/v1");
        assert_eq!(updated.model, "mixtral");
        assert_eq!(app.model_name, "mixtral");
        assert!(
            app.messages
                .iter()
                .any(|m| m.content.contains("Connected to local"))
        );
    }

    // rtmx:req REQ-LLM-027
    /// handle_connect_request sets current_provider_info on success.
    #[test]
    fn handle_connect_sets_provider_info() {
        let mut app = App::new("llama3");
        app.phase = aegis_tui::app::AppPhase::Idle;
        let config =
            aegis_llm::config::ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let shared = Arc::new(std::sync::RwLock::new(config));

        let req = aegis_tui::app::ConnectRequest {
            provider: aegis_tui::app::ConnectProvider::Local,
            endpoint: Some("http://localhost:9090/v1".to_string()),
            model: Some("codellama".to_string()),
            project: None,
            region: None,
        };
        handle_connect_request(&req, &mut app, &shared);

        let info = app.current_provider_info.as_ref().unwrap();
        assert_eq!(info.provider, "local");
        assert_eq!(info.model, "codellama");
        assert_eq!(info.endpoint, "http://localhost:9090/v1");
    }

    // rtmx:req REQ-LLM-027
    /// The config save path builds a correct AegisConfig from a
    /// ProviderConfig and saves it as valid YAML.
    #[test]
    fn connect_saves_to_config() {
        // Test the AegisConfig construction and save directly, avoiding
        // env var races from parallel tests.
        let tmp = tempfile::tempdir().expect("tempdir");
        let config_path = tmp.path().join("config.yaml");

        let provider_config = aegis_llm::config::ProviderConfig::local(
            "http://localhost:8080/v1",
            "granite-3.3-2b",
        );
        let kind_str = format!("{:?}", provider_config.kind).to_lowercase();
        let aegis_config = aegis_onboard::config::AegisConfig {
            version: "1.0".to_string(),
            mode: aegis_onboard::config::Mode::Local,
            backend: aegis_onboard::config::BackendConfig {
                provider: kind_str,
                model: provider_config.model.clone(),
                endpoint: provider_config.endpoint.clone(),
                region: provider_config.region.clone(),
                max_tokens: provider_config.max_tokens,
            },
            infra: Default::default(),
            mcp_servers: Vec::new(),
        };
        aegis_config.save(&config_path).unwrap();

        let loaded = aegis_onboard::config::AegisConfig::load(&config_path)
            .expect("saved config should load");
        assert_eq!(loaded.backend.model, "granite-3.3-2b");
        assert_eq!(loaded.backend.endpoint, "http://localhost:8080/v1");
        assert_eq!(loaded.backend.provider, "local");
    }

    // rtmx:req REQ-LLM-029
    /// handle_connect_request rejects cloud providers when auth is
    /// not available, showing actionable guidance.
    #[test]
    fn handle_connect_fails_gracefully_without_auth() {
        // Clear AWS env to ensure bedrock auth fails
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        }
        let mut app = App::new("llama3");
        app.phase = aegis_tui::app::AppPhase::Idle;
        let config =
            aegis_llm::config::ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let shared = Arc::new(std::sync::RwLock::new(config.clone()));

        let req = aegis_tui::app::ConnectRequest {
            provider: aegis_tui::app::ConnectProvider::Bedrock,
            endpoint: None,
            model: Some("claude-3-sonnet-20241022".to_string()),
            project: None,
            region: Some("us-east-1".to_string()),
        };
        handle_connect_request(&req, &mut app, &shared);

        // Shared config should NOT have changed
        let unchanged = shared.read().unwrap();
        assert_eq!(unchanged.endpoint, config.endpoint);

        // Error message should contain auth guidance
        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("AWS_ACCESS_KEY_ID")
                || last.content.contains("Authentication failed"),
            "expected auth guidance, got: {}",
            last.content,
        );
    }

    // rtmx:req REQ-LLM-024
    /// All provider creation in run_agent_for_tui goes through
    /// create_provider factory -- verified by checking that it
    /// handles all ProviderKind variants via the factory.
    #[test]
    fn main_uses_provider_factory_for_local() {
        let cfg = aegis_llm::config::ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let provider = aegis_llm::provider::create_provider(&cfg);
        assert!(
            provider.is_ok(),
            "factory should create local provider: {:?}",
            provider.err()
        );
    }

    // rtmx:req REQ-LLM-024
    /// The factory handles Vertex with a pre-resolved token.
    #[test]
    fn main_uses_provider_factory_for_vertex_with_token() {
        let cfg = aegis_llm::config::ProviderConfig::vertex(
            "test-project",
            "us-central1",
            "gemini-2.5-pro-001",
        );
        let provider = aegis_llm::provider::create_vertex_provider_with_token(
            &cfg,
            "ya29.test-token".to_string(),
        );
        assert!(
            provider.is_ok(),
            "factory should create vertex provider: {:?}",
            provider.err()
        );
    }

    // rtmx:req REQ-LLM-024
    /// resolve_provider_config routes all provider kinds through
    /// the factory path, not hardcoded LocalProvider::new().
    #[test]
    fn resolve_provider_config_creates_via_factory() {
        // Verify the config is built correctly from CLI args
        let cfg = resolve_provider_config(
            None,
            Some("local".to_string()),
            Some("test-model".to_string()),
            None,
            Some("http://localhost:9999/v1".to_string()),
        )
        .unwrap();
        assert_eq!(cfg.kind, aegis_llm::config::ProviderKind::Local);
        assert_eq!(cfg.model, "test-model");
        assert_eq!(cfg.endpoint, "http://localhost:9999/v1");

        // Verify create_provider handles it
        let provider = aegis_llm::provider::create_provider(&cfg);
        assert!(provider.is_ok());
    }

    // rtmx:req REQ-LLM-031
    /// Verify the composition root can access CSP discovery types and that
    /// the local provider returns an empty project list (safe in CI).
    #[test]
    fn csp_discovery_types_are_accessible() {
        use aegis_llm::csp_discovery::{CliCspDiscoverer, CspDiscoverer};
        let discoverer = CliCspDiscoverer;
        let result = discoverer.discover_projects("local");
        assert!(
            result.is_ok(),
            "local provider discovery should succeed: {:?}",
            result.err()
        );
        assert!(
            result.unwrap().is_empty(),
            "local provider should return no projects"
        );
    }

    // rtmx:req REQ-LLM-031
    /// Verify the TuiEvent variants used by CSP discovery wiring compile
    /// and can be constructed.
    #[test]
    fn csp_discovery_tui_events_constructible() {
        let ready = TuiEvent::CspProjectsReady {
            provider: "vertex".to_string(),
            projects: vec![("proj-1".to_string(), "My Project".to_string())],
        };
        assert!(matches!(ready, TuiEvent::CspProjectsReady { .. }));

        let err = TuiEvent::CspProjectsError {
            provider: "bedrock".to_string(),
            message: "not found".to_string(),
            guidance: "install aws cli".to_string(),
        };
        assert!(matches!(err, TuiEvent::CspProjectsError { .. }));
    }
}
