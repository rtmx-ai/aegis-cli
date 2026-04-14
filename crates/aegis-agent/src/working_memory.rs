//! In-session working memory that survives context compaction (REQ-AGENT-027).
//!
//! WorkingMemory is rendered as a System message at history index 1 (after
//! the system prompt). Since compaction preserves all System messages, the
//! memory persists across compaction cycles automatically.

use aegis_domain::ports::{Message, Role};
use aegis_domain::types::ToolCall;
use indexmap::IndexSet;

/// Marker prefix so the agent loop can find the working memory message.
pub const WORKING_MEMORY_PREFIX: &str = "[Working Memory]";

/// In-session scratchpad tracking the current task, files touched,
/// notes, and cumulative token usage.
#[derive(Debug, Clone)]
pub struct WorkingMemory {
    /// What the agent is currently working on.
    pub current_task: String,
    /// Order-preserving set of file paths the agent has interacted with.
    pub files_touched: IndexSet<String>,
    /// Free-form notes the agent can accumulate.
    pub notes: Vec<String>,
    /// Cumulative input tokens across all LLM calls in this session.
    pub cumulative_input_tokens: u64,
    /// Cumulative output tokens across all LLM calls in this session.
    pub cumulative_output_tokens: u64,
}

impl WorkingMemory {
    /// Create a new working memory initialized with the user's prompt.
    pub fn new(task: &str) -> Self {
        // Truncate the task to a reasonable summary length.
        let summary = if task.len() > 200 {
            format!("{}...", &task[..200])
        } else {
            task.to_string()
        };
        Self {
            current_task: summary,
            files_touched: IndexSet::new(),
            notes: Vec::new(),
            cumulative_input_tokens: 0,
            cumulative_output_tokens: 0,
        }
    }

    /// Render the working memory as a System message for the LLM.
    pub fn render(&self) -> Message {
        let mut content = String::from(WORKING_MEMORY_PREFIX);
        content.push_str(&format!("\nTask: {}", self.current_task));

        if !self.files_touched.is_empty() {
            let files: Vec<&str> = self.files_touched.iter().map(|s| s.as_str()).collect();
            content.push_str(&format!("\nFiles touched: {}", files.join(", ")));
        }

        if !self.notes.is_empty() {
            for note in &self.notes {
                content.push_str(&format!("\nNote: {note}"));
            }
        }

        content.push_str(&format!(
            "\nTokens used: {} in / {} out",
            self.cumulative_input_tokens, self.cumulative_output_tokens
        ));

        Message {
            role: Role::System,
            content,
        }
    }

    /// Update files_touched from a tool call's file path (if any).
    pub fn track_tool_call(&mut self, call: &ToolCall) {
        match call {
            ToolCall::ReadFile { path } => {
                self.files_touched.insert(path.to_string());
            }
            ToolCall::WriteFile { path, .. } => {
                self.files_touched.insert(path.to_string());
            }
            ToolCall::ListDir { path } => {
                self.files_touched.insert(path.to_string());
            }
            ToolCall::Grep { path, .. } => {
                self.files_touched.insert(path.to_string());
            }
            ToolCall::RunCommand { .. } | ToolCall::McpTool { .. } => {}
        }
    }

    /// Accumulate token usage from a completed LLM call.
    pub fn accumulate_tokens(&mut self, input: u64, output: u64) {
        self.cumulative_input_tokens += input;
        self.cumulative_output_tokens += output;
    }
}

/// Find the index of the working memory message in the history.
/// Returns `None` if not present.
pub fn find_memory_index(history: &[Message]) -> Option<usize> {
    history
        .iter()
        .position(|m| m.role == Role::System && m.content.starts_with(WORKING_MEMORY_PREFIX))
}

/// Update the working memory message in-place, or insert at index 1
/// if not found.
pub fn upsert_memory(history: &mut Vec<Message>, memory: &WorkingMemory) {
    let rendered = memory.render();
    if let Some(idx) = find_memory_index(history) {
        history[idx] = rendered;
    } else if !history.is_empty() {
        history.insert(1, rendered);
    } else {
        history.push(rendered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;

    // rtmx:req REQ-AGENT-027
    #[test]
    fn new_sets_task_from_prompt() {
        let wm = WorkingMemory::new("Fix the login bug");
        assert_eq!(wm.current_task, "Fix the login bug");
        assert!(wm.files_touched.is_empty());
        assert!(wm.notes.is_empty());
        assert_eq!(wm.cumulative_input_tokens, 0);
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn new_truncates_long_prompt() {
        let long = "x".repeat(500);
        let wm = WorkingMemory::new(&long);
        assert!(wm.current_task.len() <= 204); // 200 + "..."
        assert!(wm.current_task.ends_with("..."));
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn render_produces_system_message() {
        let wm = WorkingMemory::new("Refactor auth");
        let msg = wm.render();
        assert_eq!(msg.role, Role::System);
        assert!(msg.content.starts_with(WORKING_MEMORY_PREFIX));
        assert!(msg.content.contains("Task: Refactor auth"));
        assert!(msg.content.contains("Tokens used: 0 in / 0 out"));
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn render_includes_files_touched() {
        let mut wm = WorkingMemory::new("task");
        wm.files_touched.insert("src/main.rs".to_string());
        wm.files_touched.insert("src/lib.rs".to_string());
        let msg = wm.render();
        assert!(msg.content.contains("src/main.rs, src/lib.rs"));
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn render_includes_notes() {
        let mut wm = WorkingMemory::new("task");
        wm.notes.push("Found a race condition".to_string());
        let msg = wm.render();
        assert!(msg.content.contains("Note: Found a race condition"));
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn track_tool_call_records_file_paths() {
        let mut wm = WorkingMemory::new("task");
        wm.track_tool_call(&ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        });
        wm.track_tool_call(&ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/lib.rs"),
            content: "content".to_string(),
        });
        wm.track_tool_call(&ToolCall::Grep {
            pattern: "TODO".to_string(),
            path: FilePath::new_unchecked("src/"),
        });
        assert_eq!(wm.files_touched.len(), 3);
        assert!(wm.files_touched.contains("src/main.rs"));
        assert!(wm.files_touched.contains("src/lib.rs"));
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn track_tool_call_deduplicates() {
        let mut wm = WorkingMemory::new("task");
        wm.track_tool_call(&ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        });
        wm.track_tool_call(&ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        });
        assert_eq!(wm.files_touched.len(), 1);
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn track_tool_call_ignores_commands() {
        let mut wm = WorkingMemory::new("task");
        wm.track_tool_call(&ToolCall::RunCommand {
            command: "cargo test".to_string(),
            timeout_secs: 30,
        });
        assert!(wm.files_touched.is_empty());
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn accumulate_tokens_sums() {
        let mut wm = WorkingMemory::new("task");
        wm.accumulate_tokens(100, 50);
        wm.accumulate_tokens(200, 75);
        assert_eq!(wm.cumulative_input_tokens, 300);
        assert_eq!(wm.cumulative_output_tokens, 125);
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn upsert_inserts_at_index_1() {
        let wm = WorkingMemory::new("task");
        let mut history = vec![
            Message {
                role: Role::System,
                content: "You are helpful.".to_string(),
            },
            Message {
                role: Role::User,
                content: "Hello".to_string(),
            },
        ];
        upsert_memory(&mut history, &wm);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].role, Role::System); // system prompt
        assert!(history[1].content.starts_with(WORKING_MEMORY_PREFIX));
        assert_eq!(history[2].role, Role::User); // user message moved
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn upsert_updates_existing() {
        let mut wm = WorkingMemory::new("task");
        let mut history = vec![
            Message {
                role: Role::System,
                content: "You are helpful.".to_string(),
            },
            wm.render(),
            Message {
                role: Role::User,
                content: "Hello".to_string(),
            },
        ];
        wm.accumulate_tokens(100, 50);
        upsert_memory(&mut history, &wm);
        assert_eq!(history.len(), 3); // no growth
        assert!(history[1].content.contains("100 in / 50 out"));
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn working_memory_survives_compaction() {
        use crate::compaction::{CompactionConfig, compact};

        let wm = WorkingMemory::new("Implement auth");
        let mut history = vec![
            Message {
                role: Role::System,
                content: "System prompt".to_string(),
            },
            wm.render(),
        ];
        // Add many user/assistant messages to trigger compaction.
        for i in 0..20 {
            history.push(Message {
                role: Role::User,
                content: format!("User message {i}"),
            });
            history.push(Message {
                role: Role::Assistant,
                content: format!("Assistant response {i}"),
            });
        }

        let config = CompactionConfig {
            context_window: 1000,
            threshold_ratio: 0.1, // force compaction
            keep_recent: 4,
        };
        let result = compact(&history, &config);

        // Working memory should survive as a System message.
        let found = result
            .messages
            .iter()
            .any(|m| m.role == Role::System && m.content.starts_with(WORKING_MEMORY_PREFIX));
        assert!(found, "WorkingMemory should survive compaction");
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn find_memory_index_returns_correct_position() {
        let wm = WorkingMemory::new("task");
        let history = vec![
            Message {
                role: Role::System,
                content: "prompt".to_string(),
            },
            wm.render(),
            Message {
                role: Role::User,
                content: "hello".to_string(),
            },
        ];
        assert_eq!(find_memory_index(&history), Some(1));
    }

    // rtmx:req REQ-AGENT-027
    #[test]
    fn find_memory_index_returns_none_when_absent() {
        let history = vec![Message {
            role: Role::System,
            content: "prompt".to_string(),
        }];
        assert_eq!(find_memory_index(&history), None);
    }
}
