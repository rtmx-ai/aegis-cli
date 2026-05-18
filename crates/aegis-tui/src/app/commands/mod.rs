//! Slash command dispatcher and sub-handlers.

pub(crate) mod connect;
pub(crate) mod context;
pub(crate) mod cost;
pub(crate) mod doctor;
pub(crate) mod feedback;
pub(crate) mod infra;
pub(crate) mod undo;

use super::{Action, App};
use crate::messages::ChatMessage;
use crate::slash_commands::SlashCommand;
use connect::{ConnectProvider, ConnectRequest};

impl App {
    pub fn execute_slash_command(&mut self, cmd: SlashCommand) -> Action {
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
                     /model download <n>   Download model via Ollama\n\
                     /infra <subcmd>       Plugin ops: status, list, preview <name>\n\
                     /doctor               Run connectivity and health checks\n\
                     /context              Show current context summary\n\
                     /cost                 Show session cost breakdown\n\
                     /copy                 Copy last code block to clipboard\n\
                     /feedback             Submit feedback about aegis\n\
                     /undo                 Revert most recent approved write\n\
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
            // REQ-TUI-108: /model download <name> initiates Ollama pull.
            SlashCommand::ModelDownload(name) => {
                if name.is_empty() {
                    self.messages.push(ChatMessage::error(
                        "Usage: /model download <name>\n\
                         Example: /model download gemma4"
                            .to_string(),
                    ));
                    return Action::Continue;
                }
                // Check if model is already known as restricted.
                if let Some((_, status, _)) =
                    self.discovered_models.iter().find(|(id, _, _)| *id == name)
                    && status == "restricted"
                {
                    self.messages.push(ChatMessage::error(format!(
                        "Cannot download '{name}': model is restricted by origin policy"
                    )));
                    return Action::Continue;
                }
                if self.active_download.is_some() {
                    self.messages.push(ChatMessage::error(
                        "A model download is already in progress.".to_string(),
                    ));
                    return Action::Continue;
                }
                self.messages.push(ChatMessage::system(format!(
                    "Downloading model '{name}'..."
                )));
                self.active_download = Some(name.clone());
                self.pending_model_download = Some(name);
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
                    // REQ-TUI-092: validate model against discovered models
                    if !self.discovered_models.is_empty()
                        && let Some((_, status, _)) =
                            self.discovered_models.iter().find(|(id, _, _)| id == &name)
                        && status != "available"
                    {
                        let available: Vec<String> = self
                            .discovered_models
                            .iter()
                            .filter(|(_, s, _)| s == "available")
                            .map(|(id, _, _)| format!("  {id}"))
                            .collect();
                        let alt = if available.is_empty() {
                            String::new()
                        } else {
                            format!("\nAvailable models:\n{}", available.join("\n"))
                        };
                        self.messages.push(ChatMessage::error(format!(
                            "Model '{name}' is {status}.{alt}"
                        )));
                        return Action::Continue;
                    }
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
                self.handle_connect_command(&args);
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
            SlashCommand::Undo(args) => {
                self.handle_undo_command(&args);
                Action::Continue
            }
            SlashCommand::Copy => {
                self.execute_copy_command();
                Action::Continue
            }
            SlashCommand::Cost => {
                self.handle_cost_command();
                Action::Continue
            }
            SlashCommand::Feedback => {
                self.handle_feedback_command();
                Action::Continue
            }
        }
    }

    /// Handle `/connect` with parsed arguments.
    ///
    /// This method parses the arguments and either shows the current
    /// connection status (no args) or queues a `ConnectRequest` for
    /// the composition root to process.
    fn handle_connect_command(&mut self, args: &str) {
        match connect::parse_connect_args(args) {
            Err(msg) if msg.is_empty() => {
                // /connect with no args: show current provider info
                self.show_current_connection();
            }
            Err(msg) => {
                self.messages.push(ChatMessage::error(msg));
            }
            Ok(request) => {
                // For local provider with no URL, try local setup
                if request.provider == ConnectProvider::Local && request.endpoint.is_none() {
                    self.messages
                        .push(ChatMessage::system("Setting up local model...".to_string()));
                    let model = request.model.as_deref().unwrap_or("llama3");
                    match detect_and_setup_local(model) {
                        LocalSetupResult::Ready(endpoint) => {
                            self.model_name = model.to_string();
                            // Queue the resolved request for main.rs
                            let resolved = ConnectRequest {
                                provider: ConnectProvider::Local,
                                endpoint: Some(endpoint.clone()),
                                model: Some(model.to_string()),
                                project: None,
                                region: None,
                            };
                            self.pending_connect = Some(resolved);
                            self.messages.push(ChatMessage::system(format!(
                                "Ready. Ollama is running with model \
                                 '{model}' at {endpoint}.\n\
                                 You can start chatting now."
                            )));
                        }
                        LocalSetupResult::PullingModel(model_name) => {
                            self.messages.push(ChatMessage::system(format!(
                                "Failed to pull model '{model_name}' \
                                 automatically.\n\
                                 Try manually: ollama pull {model_name}\n\
                                 Then: /connect local {model_name}"
                            )));
                        }
                        LocalSetupResult::StartingServer => {
                            self.messages.push(ChatMessage::system(
                                "Ollama is installed but failed to start \
                                 automatically.\n\
                                 Try manually: ollama serve\n\
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
                                   Linux:  curl -fsSL \
                                     https://ollama.com/install.sh | sh\n\
                                 \n\
                                 Then: ollama serve && /connect local"
                                    .to_string(),
                            ));
                        }
                    }
                    return;
                }

                // Queue the connect request for the composition root.
                // Main.rs will resolve auth, probe the endpoint, save
                // config.yaml, and swap the live provider.
                let provider_label = match request.provider {
                    ConnectProvider::Local => "local endpoint",
                    ConnectProvider::Vertex => "Vertex AI",
                    ConnectProvider::Bedrock => "Bedrock",
                    ConnectProvider::Azure => "Azure OpenAI",
                };
                let detail = request
                    .endpoint
                    .as_deref()
                    .or(request.model.as_deref())
                    .unwrap_or("(default)");
                self.messages.push(ChatMessage::system(format!(
                    "Connecting to {provider_label} ({detail})..."
                )));
                self.pending_connect = Some(request);
            }
        }
    }

    /// Show current connection info: provider, model, endpoint.
    fn show_current_connection(&mut self) {
        let info = match &self.current_provider_info {
            Some(info) => format!(
                "Current connection:\n\
                 Provider: {}\n\
                 Model:    {}\n\
                 Endpoint: {}{}",
                info.provider,
                info.model,
                info.endpoint,
                info.region
                    .as_ref()
                    .map(|r| format!("\nRegion:   {r}"))
                    .unwrap_or_default(),
            ),
            None => format!(
                "Current model: {}\n\
                 No detailed provider info available.\n\
                 Use /connect <provider> to configure.",
                self.model_name
            ),
        };
        self.messages.push(ChatMessage::system(info));
    }
}

/// Provider connection info displayed by `/connect` with no args.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub region: Option<String>,
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

/// Detect the state of the local model setup and take action to fix it.
fn detect_and_setup_local(model: &str) -> LocalSetupResult {
    // Check 1: Is ollama on PATH?
    let has_ollama = std::process::Command::new("which")
        .arg("ollama")
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_ollama {
        return LocalSetupResult::NeedInstall;
    }

    // Check 2: Is ollama serve running?
    let endpoint = "http://localhost:11434";
    if !is_ollama_running(endpoint) {
        // ACT: Start ollama serve in the background
        tracing::info!("Starting ollama serve in background");
        let _ = std::process::Command::new("ollama")
            .arg("serve")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        // Wait up to 5 seconds for it to come up
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if is_ollama_running(endpoint) {
                break;
            }
        }

        if !is_ollama_running(endpoint) {
            return LocalSetupResult::StartingServer;
        }
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

    // ACT: Pull the model
    tracing::info!(model = model, "Pulling model via ollama pull");
    let pull_result = std::process::Command::new("ollama")
        .args(["pull", model])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();

    match pull_result {
        Ok(output) if output.status.success() => {
            LocalSetupResult::Ready(format!("{endpoint}/v1"))
        }
        _ => LocalSetupResult::PullingModel(model.to_string()),
    }
}

/// Check if Ollama API is responding.
fn is_ollama_running(endpoint: &str) -> bool {
    std::process::Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--connect-timeout",
            "2",
            &format!("{endpoint}/api/tags"),
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|code| code.starts_with('2'))
        .unwrap_or(false)
}
