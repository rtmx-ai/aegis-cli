//! Port traits (interfaces) for the hexagonal architecture.
//!
//! Domain crates define these traits. Infrastructure crates implement them.
//! The composition root (aegis-cli) wires implementations to ports.

use crate::error::DomainError;
use crate::event::DomainEvent;
use crate::types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Outgoing port: LLM provider for inference.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stream a response from the LLM given a conversation history and tool schemas.
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError>;

    /// Check the health of this provider (REQ-LLM-005).
    ///
    /// Returns `Healthy` if the provider responds within 1 second,
    /// `Degraded` if it responds within 5 seconds, and `Unhealthy`
    /// if it times out or returns an error.
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Unhealthy {
            message: "health_check not implemented".to_string(),
        }
    }
}

// Blanket impl: Box<dyn LlmProvider> is also an LlmProvider.
#[async_trait]
impl LlmProvider for Box<dyn LlmProvider> {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        (**self).stream(messages, tools).await
    }

    async fn health_check(&self) -> ProviderHealth {
        (**self).health_check().await
    }
}

/// A stream of tokens from an LLM response.
///
/// Uses `async_trait` to enable dyn-compatibility for `Box<dyn TokenStream>`.
#[async_trait]
pub trait TokenStream: Send + Unpin {
    /// Get the next event from the stream.
    async fn next(&mut self) -> Option<StreamEvent>;
}

/// Events emitted by an LLM token stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    Token(String),
    ToolUse(ToolCall),
    Done {
        input_tokens: u64,
        output_tokens: u64,
    },
    Error(String),
    /// A stream error with retryability classification (REQ-LLM-009).
    RetryableError {
        message: String,
        retryable: bool,
    },
}

/// Health status of an LLM provider (REQ-LLM-005).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderHealth {
    /// Provider responded within 1 second.
    Healthy { latency_ms: u64 },
    /// Provider responded between 1 and 5 seconds.
    Degraded { latency_ms: u64, message: String },
    /// Provider did not respond or returned an error.
    Unhealthy { message: String },
}

/// A message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Optional cache control marker for prompt caching (REQ-LLM-014).
    ///
    /// When set to `Some("ephemeral")`, signals to the LLM provider that
    /// this message should use prompt caching. Disabled for local providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<String>,
}

impl Message {
    /// Create a new message with no cache control.
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            cache_control: None,
        }
    }

    /// Create a new message with cache control set to "ephemeral".
    pub fn with_cache_control(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            cache_control: Some("ephemeral".to_string()),
        }
    }

    /// Returns the text content of this message (REQ-AGENT-033).
    ///
    /// Always returns the `content` field. Backwards-compatible alias.
    pub fn text_content(&self) -> &str {
        &self.content
    }
}

/// A message with optional multimodal content parts (REQ-AGENT-033).
///
/// Wraps a standard `Message` and adds structured content parts (text, images,
/// file references). Converts to/from `Message` for use with existing APIs.
///
/// Existing code continues using `Message` with its `content: String` field.
/// Multimodal-aware code constructs `MultimodalMessage` and converts to
/// `Message` (populating the text-only `content` field for backwards compat)
/// or interprets a `Message` as multimodal via `from_message()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultimodalMessage {
    /// The underlying message (role, text content, cache control).
    #[serde(flatten)]
    pub message: Message,
    /// Structured content parts. When present, these are the authoritative
    /// content; `message.content` is a text-only fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_parts: Option<Vec<ContentPart>>,
}

impl MultimodalMessage {
    /// Create a multimodal message from content parts.
    ///
    /// The `content` field on the inner `Message` is set to the concatenation
    /// of all text parts for backwards compatibility.
    pub fn with_content_parts(role: Role, parts: Vec<ContentPart>) -> Self {
        let text_content: String = parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        Self {
            message: Message::new(role, text_content),
            content_parts: Some(parts),
        }
    }

    /// Wrap an existing `Message` as a `MultimodalMessage` with no extra parts.
    pub fn from_message(msg: Message) -> Self {
        Self {
            message: msg,
            content_parts: None,
        }
    }

    /// Convert to a plain `Message`, discarding content_parts metadata.
    pub fn into_message(self) -> Message {
        self.message
    }

    /// Returns the text content (delegates to inner message).
    pub fn text_content(&self) -> &str {
        self.message.text_content()
    }

    /// Returns true if this message contains at least one image part.
    pub fn has_image(&self) -> bool {
        self.content_parts
            .as_ref()
            .is_some_and(|parts| parts.iter().any(|p| matches!(p, ContentPart::Image { .. })))
    }

    /// Returns the content parts if set, otherwise wraps `content` as a
    /// single `ContentPart::Text`.
    ///
    /// Multimodal-aware code should call this to get a uniform view of
    /// message content regardless of whether parts were explicitly set.
    pub fn content_parts_or_text(&self) -> Vec<ContentPart> {
        match &self.content_parts {
            Some(parts) => parts.clone(),
            None => vec![ContentPart::Text(self.message.content.clone())],
        }
    }
}

impl From<Message> for MultimodalMessage {
    fn from(msg: Message) -> Self {
        Self::from_message(msg)
    }
}

impl From<MultimodalMessage> for Message {
    fn from(mm: MultimodalMessage) -> Self {
        mm.into_message()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

/// JSON schema for a tool the LLM can call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Outgoing port: HITL approval gate.
#[async_trait]
pub trait ApprovalGate: Send + Sync {
    /// Request human approval for a tool call. Blocks until the user responds.
    async fn request_approval(
        &self,
        tool_call: &ToolCall,
    ) -> Result<ApprovalDecision, DomainError>;
}

/// Outgoing port: audit ledger.
#[async_trait]
pub trait AuditLedger: Send + Sync {
    /// Append an event to the immutable audit ledger.
    async fn record(&self, event: &DomainEvent) -> Result<(), DomainError>;

    /// Append an event linked to a specific RTMX requirement ID (REQ-AUDIT-003).
    /// Default delegates to `record()` for backward compatibility.
    async fn record_with_req(
        &self,
        event: &DomainEvent,
        _req_id: Option<&str>,
    ) -> Result<(), DomainError> {
        self.record(event).await
    }
}

// Blanket impl: Arc<T: AuditLedger> is also an AuditLedger.
#[async_trait]
impl<T: AuditLedger> AuditLedger for std::sync::Arc<T> {
    async fn record(&self, event: &DomainEvent) -> Result<(), DomainError> {
        (**self).record(event).await
    }

    async fn record_with_req(
        &self,
        event: &DomainEvent,
        req_id: Option<&str>,
    ) -> Result<(), DomainError> {
        (**self).record_with_req(event, req_id).await
    }
}

/// Outgoing port: security filter (.aegisignore).
pub trait SecurityFilter: Send + Sync {
    /// Check if a file path is blocked by .aegisignore.
    fn is_blocked(&self, path: &str) -> bool;

    /// Validate and wrap a path, returning an error if blocked.
    fn validate_path(&self, path: &str) -> Result<FilePath, DomainError>;
}

// Blanket impl: Arc<T: SecurityFilter> is also a SecurityFilter.
impl<T: SecurityFilter> SecurityFilter for std::sync::Arc<T> {
    fn is_blocked(&self, path: &str) -> bool {
        (**self).is_blocked(path)
    }

    fn validate_path(&self, path: &str) -> Result<FilePath, DomainError> {
        (**self).validate_path(path)
    }
}

/// Outgoing port: tool executor.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool call and return the result.
    async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-014
    #[test]
    fn message_new_has_no_cache_control() {
        let msg = Message::new(Role::System, "You are helpful.");
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.content, "You are helpful.");
        assert!(msg.cache_control.is_none());
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn message_with_cache_control_sets_ephemeral() {
        let msg = Message::with_cache_control(Role::System, "You are helpful.");
        assert_eq!(msg.cache_control, Some("ephemeral".to_string()));
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn message_cache_control_defaults_to_none_on_deserialize() {
        let json = r#"{"role":"System","content":"hello"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.cache_control.is_none());
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn message_cache_control_skipped_in_serialization_when_none() {
        let msg = Message::new(Role::User, "hello");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(
            !json.contains("cache_control"),
            "cache_control should be skipped when None: {json}"
        );
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn message_cache_control_included_in_serialization_when_some() {
        let msg = Message::with_cache_control(Role::System, "prompt");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(
            json.contains("cache_control"),
            "cache_control should be present when Some: {json}"
        );
        assert!(json.contains("ephemeral"));
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn content_part_text_from_str() {
        let part: ContentPart = "hello".into();
        assert_eq!(part, ContentPart::Text("hello".to_string()));
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn content_part_text_from_string() {
        let part: ContentPart = String::from("world").into();
        assert_eq!(part, ContentPart::Text("world".to_string()));
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn content_part_image_construction() {
        let part = ContentPart::Image {
            mime: "image/png".to_string(),
            data: vec![0x89, 0x50, 0x4E, 0x47],
        };
        match &part {
            ContentPart::Image { mime, data } => {
                assert_eq!(mime, "image/png");
                assert_eq!(data, &[0x89, 0x50, 0x4E, 0x47]);
            }
            _ => panic!("expected Image variant"),
        }
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn content_part_file_ref_construction() {
        let part = ContentPart::FileRef(std::path::PathBuf::from("/tmp/diagram.svg"));
        match &part {
            ContentPart::FileRef(p) => {
                assert_eq!(p, &std::path::PathBuf::from("/tmp/diagram.svg"));
            }
            _ => panic!("expected FileRef variant"),
        }
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn multimodal_message_with_content_parts_returns_parts() {
        let parts = vec![
            ContentPart::Text("Look at this:".to_string()),
            ContentPart::Image {
                mime: "image/png".to_string(),
                data: vec![1, 2, 3],
            },
        ];
        let mm = MultimodalMessage::with_content_parts(Role::User, parts.clone());
        assert_eq!(mm.content_parts_or_text(), parts);
        // inner message content has text portion for backwards compat
        assert_eq!(mm.message.content, "Look at this:");
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn multimodal_message_without_content_parts_wraps_content_as_text() {
        let msg = Message::new(Role::User, "plain text");
        let mm = MultimodalMessage::from_message(msg);
        let parts = mm.content_parts_or_text();
        assert_eq!(parts, vec![ContentPart::Text("plain text".to_string())]);
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn has_image_returns_true_when_image_present() {
        let mm = MultimodalMessage::with_content_parts(
            Role::User,
            vec![
                ContentPart::Text("see image".to_string()),
                ContentPart::Image {
                    mime: "image/jpeg".to_string(),
                    data: vec![0xFF, 0xD8],
                },
            ],
        );
        assert!(mm.has_image());
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn has_image_returns_false_for_text_only() {
        let msg = Message::new(Role::User, "no images here");
        let mm = MultimodalMessage::from(msg);
        assert!(!mm.has_image());

        let mm2 = MultimodalMessage::with_content_parts(
            Role::User,
            vec![ContentPart::Text("still no images".to_string())],
        );
        assert!(!mm2.has_image());
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn content_part_serde_roundtrip() {
        let parts = vec![
            ContentPart::Text("hello".to_string()),
            ContentPart::Image {
                mime: "image/png".to_string(),
                data: vec![1, 2, 3, 4],
            },
            ContentPart::FileRef(std::path::PathBuf::from("/tmp/file.txt")),
        ];
        let json = serde_json::to_string(&parts).unwrap();
        let deserialized: Vec<ContentPart> = serde_json::from_str(&json).unwrap();
        assert_eq!(parts, deserialized);
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn multimodal_message_content_parts_defaults_to_none_on_deserialize() {
        let json = r#"{"role":"User","content":"hello"}"#;
        let mm: MultimodalMessage = serde_json::from_str(json).unwrap();
        assert!(mm.content_parts.is_none());
        assert_eq!(mm.message.content, "hello");
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn multimodal_message_content_parts_skipped_in_serialization_when_none() {
        let mm = MultimodalMessage::from_message(Message::new(Role::User, "hello"));
        let json = serde_json::to_string(&mm).unwrap();
        assert!(
            !json.contains("content_parts"),
            "content_parts should be skipped when None: {json}"
        );
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn multimodal_message_converts_to_and_from_message() {
        let original = Message::new(Role::Assistant, "response text");
        let mm = MultimodalMessage::from(original.clone());
        let back: Message = mm.into();
        assert_eq!(back, original);
    }

    // rtmx:req REQ-AGENT-033
    #[test]
    fn message_text_content_returns_content_field() {
        let msg = Message::new(Role::User, "hello world");
        assert_eq!(msg.text_content(), "hello world");
    }
}
