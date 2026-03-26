//! aegis-cli: Terminal-native agentic AI pair programmer for CUI
//! environments.

use aegis_domain::error::DomainError;
use aegis_domain::ports::ApprovalGate;
use aegis_domain::types::{ApprovalDecision, ToolCall};
use async_trait::async_trait;
use clap::Parser;
use std::sync::Arc;

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
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

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
            eprintln!(
                "aegis: use --help for usage. \
                 Start with: aegis init --local"
            );
            std::process::exit(0);
        }
    };

    if let Err(e) = result {
        eprintln!("aegis: {e}");
        std::process::exit(1);
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
    let prompt =
        prompt.ok_or_else(|| "Prompt required. Use: aegis chat -p 'your prompt'".to_string())?;

    if !headless {
        return Err("Interactive TUI not yet wired. \
             Use: aegis chat --headless -p 'prompt'"
            .to_string());
    }

    // Load config or use defaults
    let (endpoint, model) = if let Some(ep) = local_endpoint {
        (ep, "default".to_string())
    } else {
        let config_path =
            aegis_onboard::config::AegisConfig::default_path().map_err(|e| e.to_string())?;
        let config = aegis_onboard::config::AegisConfig::load(&config_path)
            .map_err(|e| format!("No config found. Run 'aegis init --local' first. ({e})"))?;
        (config.backend.endpoint, config.backend.model)
    };

    // Create the runtime and run the agent
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;

    rt.block_on(async { run_headless_chat(&prompt, &endpoint, &model).await })
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
