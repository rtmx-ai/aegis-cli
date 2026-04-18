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
