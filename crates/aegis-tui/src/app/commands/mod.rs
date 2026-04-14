//! Slash command dispatcher and sub-handlers.

pub(crate) mod context;
pub(crate) mod doctor;
pub(crate) mod infra;

use super::{Action, App};
use crate::messages::ChatMessage;
use crate::slash_commands::SlashCommand;

impl App {
    pub(crate) fn execute_slash_command(&mut self, cmd: SlashCommand) -> Action {
        match cmd {
            SlashCommand::Clear => {
                self.messages.clear();
                self.scroll_offset = 0;
                Action::Continue
            }
            SlashCommand::Help => {
                self.messages.push(ChatMessage::system(
                    "Commands:\n\
                     /connect <provider>   Connect to an LLM provider\n\
                     /add <path>           Add file to context\n\
                     /drop <path>          Remove file from context\n\
                     /model [name]         Show or switch active model\n\
                     /infra <subcmd>       Plugin ops: status, list, preview <name>\n\
                     /doctor               Run connectivity and health checks\n\
                     /context              Show current context summary\n\
                     /clear                Clear chat log\n\
                     /help                 Show this help\n\
                     /quit                 Exit aegis\n\
                     \n\
                     Shortcuts: Ctrl+C quit, Esc vim mode\n\
                     Approval: [A]pprove [D]eny [E]dit [S]kip\n\
                     \n\
                     aegis blocks writes until you approve. Read-only tools auto-execute."
                        .to_string(),
                ));
                Action::Continue
            }
            SlashCommand::Context => self.handle_context_command(),
            SlashCommand::Quit => Action::Quit,
            SlashCommand::Add(path) => self.handle_add_command(path),
            SlashCommand::Drop(path) => self.handle_drop_command(path),
            SlashCommand::Infra(sub) => {
                self.handle_infra_command(&sub);
                Action::Continue
            }
            SlashCommand::Doctor => {
                self.handle_doctor_command();
                Action::Continue
            }
            SlashCommand::Model(name) => {
                if name.is_empty() {
                    tracing::info!(model = %self.model_name, "displaying current model");
                    self.messages.push(ChatMessage::system(format!(
                        "Current model: {}",
                        self.model_name
                    )));
                } else {
                    tracing::info!(
                        old = %self.model_name,
                        new = %name,
                        "switching model"
                    );
                    self.model_name = name.clone();
                    self.messages
                        .push(ChatMessage::system(format!("Model switched to: {name}")));
                }
                Action::Continue
            }
            SlashCommand::Connect(args) => {
                let parts: Vec<&str> = args.split_whitespace().collect();
                let provider = parts.first().copied().unwrap_or("");
                match provider {
                    "" => {
                        self.messages.push(ChatMessage::system(
                            "Connect to an LLM provider:\n\
                             \n\
                             Local models:\n\
                               /connect local              Ollama (auto-installs llama3)\n\
                               /connect local <url>        Connect to running endpoint\n\
                             \n\
                             Cloud providers:\n\
                               /connect vertex             Google Vertex AI (Gemini)\n\
                               /connect bedrock            AWS Bedrock (Claude)\n\
                               /connect azure <endpoint>   Azure OpenAI\n\
                             \n\
                             Infrastructure provisioning:\n\
                               /infra list                 Show available plugins\n\
                               /infra up <plugin>          Provision cloud environment\n\
                             \n\
                             Direct endpoint:\n\
                               /connect http://...         Any OpenAI-compatible API"
                                .to_string(),
                        ));
                    }
                    "local" => {
                        let model = parts.get(1).copied().unwrap_or("llama3");
                        self.messages.push(ChatMessage::system(format!(
                            "Setting up local model '{model}'..."
                        )));
                        match detect_and_setup_local(model) {
                            LocalSetupResult::Ready(endpoint) => {
                                self.messages.push(ChatMessage::system(format!(
                                    "Connected to {endpoint} with model '{model}'.\n\
                                     Restart aegis to use: aegis chat --provider local --model {model}"
                                )));
                            }
                            LocalSetupResult::PullingModel(model_name) => {
                                self.messages.push(ChatMessage::system(format!(
                                    "Ollama is running but model '{model_name}' is not pulled.\n\
                                     Run in another terminal: ollama pull {model_name}\n\
                                     Then: /connect local {model_name}"
                                )));
                            }
                            LocalSetupResult::StartingServer => {
                                self.messages.push(ChatMessage::system(
                                    "Ollama is installed but not running.\n\
                                     Run in another terminal: ollama serve\n\
                                     Then: /connect local"
                                        .to_string(),
                                ));
                            }
                            LocalSetupResult::NeedInstall => {
                                self.messages.push(ChatMessage::system(
                                    "Ollama is not installed.\n\
                                     \n\
                                     Install with:\n\
                                       macOS:  brew install ollama\n\
                                       Linux:  curl -fsSL https://ollama.com/install.sh | sh\n\
                                     \n\
                                     Then: ollama serve && /connect local"
                                        .to_string(),
                                ));
                            }
                        }
                    }
                    "vertex" => {
                        self.messages.push(ChatMessage::system(
                            "Connecting to Google Vertex AI...\n\
                             \n\
                             Requires: gcloud auth application-default login\n\
                             Restart aegis with: aegis chat --provider vertex"
                                .to_string(),
                        ));
                    }
                    "bedrock" => {
                        let region = parts.get(1).copied().unwrap_or("us-east-1");
                        self.messages.push(ChatMessage::system(format!(
                            "Connecting to AWS Bedrock ({region})...\n\
                             \n\
                             Requires: AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY\n\
                             Restart aegis with: aegis chat --provider bedrock --region {region}"
                        )));
                    }
                    "azure" => {
                        let endpoint = parts.get(1).copied().unwrap_or("");
                        if endpoint.is_empty() {
                            self.messages.push(ChatMessage::error(
                                "Azure requires an endpoint URL:\n\
                                 /connect azure https://myresource.openai.azure.com"
                                    .to_string(),
                            ));
                        } else {
                            self.messages.push(ChatMessage::system(format!(
                                "Connecting to Azure OpenAI at {endpoint}...\n\
                                 \n\
                                 Requires: AZURE_OPENAI_API_KEY\n\
                                 Restart aegis with: aegis chat --provider azure --endpoint {endpoint}"
                            )));
                        }
                    }
                    url if url.starts_with("http") => {
                        tracing::info!(endpoint = %url, "connecting to LLM endpoint");
                        self.messages.push(ChatMessage::system(format!(
                            "Endpoint set to: {url}\n\
                             Type a message to test the connection."
                        )));
                    }
                    other => {
                        self.messages.push(ChatMessage::error(format!(
                            "Unknown provider '{other}'.\n\
                             Options: local, vertex, bedrock, azure\n\
                             Or: /connect http://... for direct endpoint"
                        )));
                    }
                }
                Action::Continue
            }
            SlashCommand::KeyLog => {
                self.keylog = !self.keylog;
                let state = if self.keylog { "ON" } else { "OFF" };
                self.messages.push(ChatMessage::system(format!(
                    "Key event logging: {state}. Press keys to see raw events."
                )));
                Action::Continue
            }
        }
    }
}

/// Result of detecting local model setup state.
enum LocalSetupResult {
    /// Ollama is running and model is available.
    Ready(String),
    /// Ollama is running but model needs to be pulled.
    PullingModel(String),
    /// Ollama is installed but the server is not running.
    StartingServer,
    /// Ollama is not installed.
    NeedInstall,
}

/// Detect the state of the local model setup and return the appropriate action.
fn detect_and_setup_local(model: &str) -> LocalSetupResult {
    // Check 1: Is ollama on PATH?
    let ollama_path = std::process::Command::new("which")
        .arg("ollama")
        .output()
        .ok()
        .filter(|o| o.status.success());

    if ollama_path.is_none() {
        return LocalSetupResult::NeedInstall;
    }

    // Check 2: Is ollama serve running? (probe the API)
    let endpoint = "http://localhost:11434";
    let probe = std::process::Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            &format!("{endpoint}/api/tags"),
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok());

    let server_up = probe
        .as_deref()
        .map(|code| code.starts_with('2'))
        .unwrap_or(false);

    if !server_up {
        return LocalSetupResult::StartingServer;
    }

    // Check 3: Is the requested model already pulled?
    let tags = std::process::Command::new("curl")
        .args(["-s", &format!("{endpoint}/api/tags")])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    if tags.contains(model) {
        return LocalSetupResult::Ready(format!("{endpoint}/v1"));
    }

    // Model not pulled yet
    LocalSetupResult::PullingModel(model.to_string())
}
