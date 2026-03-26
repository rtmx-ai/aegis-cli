//! aegis-cli: Terminal-native agentic AI pair programmer for CUI
//! environments.

use clap::Parser;

/// Build version with embedded git SHA and target.
fn long_version() -> &'static str {
    // Leak a String to get a &'static str for clap.
    // Called once at startup -- acceptable.
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
        /// Configure for air-gapped operation with local
        /// models only
        #[arg(long)]
        local: bool,
    },
    /// Start an interactive chat session
    Chat {
        /// Initial prompt (optional, enters REPL if omitted)
        prompt: Option<String>,
    },
    /// Check infrastructure health via plugin status
    Doctor,
}

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Init { local }) => run_init(local),
        Some(Commands::Chat { prompt: _ }) => {
            eprintln!(
                "aegis chat: TUI not yet wired. \
                 Use aegis init --local first."
            );
            std::process::exit(1);
        }
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
