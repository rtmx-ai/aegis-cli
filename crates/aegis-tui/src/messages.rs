//! Chat message types for TUI rendering.

/// A rendered message in the chat log.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub kind: MessageKind,
    pub content: String,
}

/// Kind of chat message, determines rendering style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageKind {
    User,
    Assistant,
    ToolCall { tool_name: String },
    ToolResult,
    Error,
    System,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::Assistant,
            content: content.into(),
        }
    }

    pub fn tool_call(tool_name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::ToolCall {
                tool_name: tool_name.into(),
            },
            content: detail.into(),
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::Error,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::System,
            content: content.into(),
        }
    }

    pub fn tool_result(content: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::ToolResult,
            content: content.into(),
        }
    }
}
