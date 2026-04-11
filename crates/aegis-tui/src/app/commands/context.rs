//! /add, /drop, /context command handlers.

use crate::app::{Action, App};
use crate::messages::ChatMessage;
use std::path::PathBuf;

impl App {
    pub(crate) fn handle_add_command(&mut self, path: String) -> Action {
        if path.is_empty() {
            self.messages
                .push(ChatMessage::error("Usage: /add <path>".to_string()));
            return Action::Continue;
        }
        let pb = PathBuf::from(&path);
        if !pb.exists() {
            self.messages
                .push(ChatMessage::error(format!("File not found: {path}")));
            return Action::Continue;
        }
        if self.context_files.contains(&pb) {
            self.messages
                .push(ChatMessage::system(format!("Already in context: {path}")));
            return Action::Continue;
        }
        self.context_files.push(pb);
        self.messages
            .push(ChatMessage::system(format!("Added to context: {path}")));
        Action::Continue
    }

    pub(crate) fn handle_drop_command(&mut self, path: String) -> Action {
        if path.is_empty() {
            self.messages
                .push(ChatMessage::error("Usage: /drop <path>".to_string()));
            return Action::Continue;
        }
        let pb = PathBuf::from(&path);
        if let Some(pos) = self.context_files.iter().position(|p| p == &pb) {
            self.context_files.remove(pos);
            self.messages
                .push(ChatMessage::system(format!("Removed from context: {path}")));
        } else {
            self.messages
                .push(ChatMessage::error(format!("Not in context: {path}")));
        }
        Action::Continue
    }

    pub(crate) fn handle_context_command(&mut self) -> Action {
        let files_info = if self.context_files.is_empty() {
            "Context files: (none)".to_string()
        } else {
            let paths: Vec<String> = self
                .context_files
                .iter()
                .map(|p| format!("  {}", p.display()))
                .collect();
            format!("Context files:\n{}", paths.join("\n"))
        };
        self.messages.push(ChatMessage::system(format!(
            "Model: {}\nTokens: {}in + {}out\nMessages: {}\n{}",
            self.model_name,
            self.input_tokens,
            self.output_tokens,
            self.messages.len(),
            files_info,
        )));
        Action::Continue
    }
}
