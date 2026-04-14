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
                        let endpoint =
                            parts.get(1).copied().unwrap_or("http://localhost:11434/v1");
                        self.messages.push(ChatMessage::system(format!(
                            "Connecting to local model at {endpoint}...\n\
                             \n\
                             If Ollama is not running:\n\
                               brew install ollama\n\
                               ollama serve\n\
                               ollama pull llama3\n\
                             \n\
                             Then restart aegis or run: /connect local"
                        )));
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
