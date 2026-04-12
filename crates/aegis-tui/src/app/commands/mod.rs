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
                     /connect <url>        Connect to an LLM endpoint\n\
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
            SlashCommand::Connect(url) => {
                if url.is_empty() {
                    self.messages.push(ChatMessage::system(format!(
                        "Current endpoint: http://localhost:11434/v1\n\
                         Current model: {}\n\n\
                         Usage: /connect <endpoint-url>",
                        self.model_name
                    )));
                } else {
                    // Validate URL format
                    if !url.starts_with("http://") && !url.starts_with("https://") {
                        self.messages.push(ChatMessage::error(
                            "Endpoint must start with http:// or https://".to_string(),
                        ));
                    } else {
                        tracing::info!(endpoint = %url, "connecting to LLM endpoint");
                        self.messages.push(ChatMessage::system(format!(
                            "Endpoint set to: {url}\n\
                             Type a message to test the connection."
                        )));
                        // TODO: reconfigure the agent's provider at runtime
                        // For now, the user needs to restart aegis after /connect
                        // to pick up the new endpoint from config.
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
