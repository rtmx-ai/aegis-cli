//! /infra subcommand handler.

use crate::app::App;
use crate::messages::ChatMessage;

impl App {
    /// Handle /infra subcommands: status, list, preview <name>.
    pub(crate) fn handle_infra_command(&mut self, sub: &str) {
        let parts: Vec<&str> = sub.split_whitespace().collect();
        let subcmd = parts.first().copied().unwrap_or("");

        match subcmd {
            "status" => {
                self.messages.push(ChatMessage::system(
                    "[infra status] No plugins discovered. \
                     Install aegis-infra/v1 plugins on PATH to enable."
                        .to_string(),
                ));
            }
            "list" => {
                self.messages.push(ChatMessage::system(
                    "[infra list] No aegis-infra/v1 plugins found on PATH.".to_string(),
                ));
            }
            "preview" => {
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::error(
                        "Usage: /infra preview <plugin-name>".to_string(),
                    ));
                } else {
                    let plugin_name = parts[1];
                    self.messages.push(ChatMessage::system(format!(
                        "[infra preview] Plugin '{plugin_name}' not found. \
                         Run /infra list to see available plugins."
                    )));
                }
            }
            "" => {
                self.messages.push(ChatMessage::system(
                    "Usage: /infra <status|list|preview <name>>".to_string(),
                ));
            }
            other => {
                self.messages.push(ChatMessage::error(format!(
                    "Unknown /infra subcommand: {other}. \
                     Try: status, list, preview"
                )));
            }
        }
    }
}
