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
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ToolUse(ToolCall),
    Done {
        input_tokens: u64,
        output_tokens: u64,
    },
    Error(String),
}

/// A message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    async fn request_approval(&self, tool_call: &ToolCall)
    -> Result<ApprovalDecision, DomainError>;
}

/// Outgoing port: audit ledger.
#[async_trait]
pub trait AuditLedger: Send + Sync {
    /// Append an event to the immutable audit ledger.
    async fn record(&self, event: &DomainEvent) -> Result<(), DomainError>;
}

/// Outgoing port: security filter (.aegisignore).
pub trait SecurityFilter: Send + Sync {
    /// Check if a file path is blocked by .aegisignore.
    fn is_blocked(&self, path: &str) -> bool;

    /// Validate and wrap a path, returning an error if blocked.
    fn validate_path(&self, path: &str) -> Result<FilePath, DomainError>;
}

/// Outgoing port: tool executor.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool call and return the result.
    async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult, DomainError>;
}
