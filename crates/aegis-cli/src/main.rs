//! aegis: Terminal-native agentic AI pair programmer for CUI environments.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "aegis",
    version,
    about = "Agentic AI pair programmer for CUI environments"
)]
struct Cli {
    /// Initialize cloud infrastructure and local configuration
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
    /// Start an interactive session
    Chat {
        /// Initial prompt (optional, enters REPL if omitted)
        prompt: Option<String>,
    },
}

fn main() {
    let _cli = Cli::parse();

    // TODO: Wire bounded contexts together (composition root)
    // 1. Parse config from ~/.aegis/config.yaml
    // 2. Instantiate provider based on config
    // 3. Instantiate security filter, HITL gate, audit ledger, tool executor
    // 4. Instantiate agent loop with all ports
    // 5. Launch TUI with agent loop

    eprintln!("aegis: not yet implemented");
    std::process::exit(1);
}
