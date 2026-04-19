//! `/undo` slash command: rollback approved writes via RollbackJournal.
//!
//! rtmx:req REQ-TUI-027

use super::App;
use crate::messages::ChatMessage;

/// Parsed undo request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoRequest {
    Last,
    All,
    Specific(u64),
}

pub fn parse_undo_args(args: &str) -> UndoRequest {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return UndoRequest::Last;
    }
    if trimmed == "--all" {
        return UndoRequest::All;
    }
    match trimmed.parse::<u64>() {
        Ok(id) => UndoRequest::Specific(id),
        Err(_) => UndoRequest::Last,
    }
}

impl App {
    pub(crate) fn handle_undo_command(&mut self, args: &str) {
        let request = parse_undo_args(args);

        if self.rollback_journal.is_empty() {
            self.messages.push(ChatMessage::system(
                "Nothing to undo. The rollback journal is empty.".to_string(),
            ));
            return;
        }

        match request {
            UndoRequest::Last => match self.rollback_journal.rollback_last() {
                Ok(paths) => {
                    let path_list: Vec<String> =
                        paths.iter().map(|p| p.display().to_string()).collect();
                    self.messages.push(ChatMessage::system(format!(
                        "Rolled back last write. Restored: {}",
                        path_list.join(", ")
                    )));
                }
                Err(e) => {
                    self.messages
                        .push(ChatMessage::error(format!("Undo failed: {e}")));
                }
            },
            UndoRequest::All => {
                let mut total_restored = Vec::new();
                while !self.rollback_journal.is_empty() {
                    match self.rollback_journal.rollback_last() {
                        Ok(paths) => total_restored.extend(paths),
                        Err(e) => {
                            self.messages
                                .push(ChatMessage::error(format!("Undo (all) failed: {e}")));
                            return;
                        }
                    }
                }
                self.messages.push(ChatMessage::system(format!(
                    "Rolled back all entries. Restored {} file(s).",
                    total_restored.len()
                )));
            }
            UndoRequest::Specific(id) => match self.rollback_journal.rollback(id) {
                Ok(paths) => {
                    let path_list: Vec<String> =
                        paths.iter().map(|p| p.display().to_string()).collect();
                    self.messages.push(ChatMessage::system(format!(
                        "Rolled back entry {id}. Restored: {}",
                        path_list.join(", ")
                    )));
                }
                Err(e) => {
                    self.messages
                        .push(ChatMessage::error(format!("Undo failed: {e}")));
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_palette::CommandPalette;

    // rtmx:req REQ-TUI-027
    #[test]
    fn test_parse_undo_empty_is_last() {
        assert_eq!(parse_undo_args(""), UndoRequest::Last);
    }

    // rtmx:req REQ-TUI-027
    #[test]
    fn test_parse_undo_all_flag() {
        assert_eq!(parse_undo_args("--all"), UndoRequest::All);
    }

    // rtmx:req REQ-TUI-027
    #[test]
    fn test_parse_undo_specific_id() {
        assert_eq!(parse_undo_args("42"), UndoRequest::Specific(42));
    }

    // rtmx:req REQ-TUI-027
    #[test]
    fn test_parse_undo_invalid_falls_back() {
        assert_eq!(parse_undo_args("abc"), UndoRequest::Last);
    }

    // rtmx:req REQ-TUI-027
    #[test]
    fn test_undo_command_registered_in_palette() {
        let palette = CommandPalette::new();
        let has_undo = palette.filtered.iter().any(|e| e.name == "/undo");
        assert!(has_undo, "/undo should be in the command palette");
    }
}
