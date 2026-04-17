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

/// Build a structured welcome message shown after the splash screen dismisses.
///
/// Displays provider, model, and connection health so the user immediately
/// knows whether the LLM backend is reachable. If the provider is not
/// connected, the message includes `/connect` guidance.
///
/// `model` is the model display name (e.g. "gemini-2.5-pro", "llama3").
/// `status` is a human-readable connection status string (e.g.
/// "Connected (latency: 450ms)" or "Not connected").
pub fn build_welcome_message(model: &str, status: &str) -> ChatMessage {
    let version = crate::brand::VERSION;

    let connected = status.to_lowercase().contains("connected")
        && !status.to_lowercase().contains("not connected");

    let status_line = if connected {
        format!("  Status:    {status}")
    } else {
        format!(
            "  Status:    {status}\n\n  \
             Run /connect to configure a provider, or use `aegis init --local` for air-gapped mode."
        )
    };

    let provider_line = format!("  Provider:  {model}");
    let hint = "  Type a message or use /help for commands.";

    let body = format!(
        "  aegis v{version} -- terminal-native AI pair programmer\n\
         \n\
         {provider_line}\n\
         {status_line}\n\
         \n\
         {hint}",
    );

    ChatMessage::system(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-057
    #[test]
    fn test_welcome_message_contains_provider_info() {
        let msg = build_welcome_message("gemini-2.5-pro", "Connected (latency: 450ms)");
        assert!(
            msg.content.contains("gemini-2.5-pro"),
            "should contain model name: {}",
            msg.content
        );
        assert!(
            msg.content.contains("Connected"),
            "should contain status: {}",
            msg.content
        );
        assert!(
            msg.content.contains("aegis v"),
            "should contain version header: {}",
            msg.content
        );
        assert!(
            msg.content.contains("/help"),
            "should contain /help hint: {}",
            msg.content
        );
        assert!(
            !msg.content.contains("/connect"),
            "connected state should not show /connect guidance: {}",
            msg.content
        );
        assert_eq!(msg.kind, MessageKind::System);
    }

    // rtmx:req REQ-TUI-057
    #[test]
    fn test_welcome_message_for_disconnected_state() {
        let msg = build_welcome_message("none", "Not connected");
        assert!(
            msg.content.contains("none"),
            "should contain model placeholder: {}",
            msg.content
        );
        assert!(
            msg.content.contains("Not connected"),
            "should contain disconnected status: {}",
            msg.content
        );
        assert!(
            msg.content.contains("/connect"),
            "disconnected state should show /connect guidance: {}",
            msg.content
        );
        assert_eq!(msg.kind, MessageKind::System);
    }

    // rtmx:req REQ-TUI-057
    #[test]
    fn test_welcome_message_contains_version() {
        let msg = build_welcome_message("llama3", "Connected");
        let expected_version = format!("aegis v{}", crate::brand::VERSION);
        assert!(
            msg.content.contains(&expected_version),
            "should contain version string '{}': {}",
            expected_version,
            msg.content
        );
    }

    // rtmx:req REQ-TUI-057
    #[test]
    fn test_welcome_message_hint_present() {
        let msg = build_welcome_message("llama3", "Connected");
        assert!(
            msg.content.contains("Type a message"),
            "should contain usage hint: {}",
            msg.content
        );
    }
}
