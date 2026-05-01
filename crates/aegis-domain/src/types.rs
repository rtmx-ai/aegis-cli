//! Domain value objects with compile-time safety guarantees.
//!
//! Newtypes prevent accidental misuse (e.g., passing a SessionId where a
//! RequestId is expected). Construction validates invariants.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for an agent session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl Default for SessionId {
    fn default() -> Self {
        Self(Uuid::new_v4())
    }
}

impl SessionId {
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a single request within a session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(Uuid);

impl Default for RequestId {
    fn default() -> Self {
        Self(Uuid::new_v4())
    }
}

impl RequestId {
    pub fn new() -> Self {
        Self::default()
    }
}

/// An RTMX requirement identifier (e.g., "REQ-BUILD-001").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequirementId(String);

impl RequirementId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequirementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated file path that has been checked against .aegisignore.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FilePath(PathBuf);

impl FilePath {
    /// Create a FilePath. In production, this should go through the
    /// SecurityFilter port to validate against .aegisignore.
    pub fn new_unchecked(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &std::path::Path {
        &self.0
    }
}

impl fmt::Display for FilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// A content part within a message, supporting multimodal content (REQ-AGENT-033).
///
/// Messages can contain text, images, or file references. This enum enables
/// multimodal conversations while remaining backwards-compatible with the
/// existing `Message.content: String` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentPart {
    /// Plain text content.
    Text(String),
    /// Inline image with MIME type and raw bytes.
    Image { mime: String, data: Vec<u8> },
    /// Reference to a file on disk.
    FileRef(PathBuf),
}

impl From<&str> for ContentPart {
    fn from(s: &str) -> Self {
        ContentPart::Text(s.to_string())
    }
}

impl From<String> for ContentPart {
    fn from(s: String) -> Self {
        ContentPart::Text(s)
    }
}

/// A tool call the agent wants to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCall {
    ReadFile {
        path: FilePath,
    },
    WriteFile {
        path: FilePath,
        content: String,
    },
    RunCommand {
        command: String,
        timeout_secs: u64,
    },
    ListDir {
        path: FilePath,
    },
    Grep {
        pattern: String,
        path: FilePath,
    },
    /// MCP tool call routed to an external server (REQ-AGENT-014).
    McpTool {
        qualified_name: String,
        arguments: serde_json::Value,
    },
}

/// Risk classification for tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    ReadOnly,
    StateMutating,
}

impl ToolCall {
    pub fn risk(&self) -> ToolRisk {
        match self {
            Self::ReadFile { .. } | Self::ListDir { .. } | Self::Grep { .. } => {
                ToolRisk::ReadOnly
            }
            Self::WriteFile { .. } | Self::RunCommand { .. } | Self::McpTool { .. } => {
                ToolRisk::StateMutating
            }
        }
    }
}

/// The result of executing a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolResult {
    Success { output: String },
    Error { message: String },
    PermissionDenied { reason: String },
}

/// HITL approval decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Approved,
    Denied,
    Edited,
    Skipped,
    /// Auto-denied because the HITL timeout expired (REQ-HITL-003).
    TimedOut,
}

// ---------------------------------------------------------------------------
// MCP server configuration (REQ-AGENT-022)
// ---------------------------------------------------------------------------

/// Configuration for an MCP server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable name for this server.
    pub name: String,
    /// Transport configuration.
    pub transport: McpTransport,
}

/// Transport type for connecting to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransport {
    /// Spawn a subprocess, communicate via stdin/stdout JSON-RPC (NDJSON).
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
    },
    /// Connect via HTTP+SSE (not yet implemented).
    Sse {
        url: String,
        #[serde(default)]
        headers: std::collections::HashMap<String, String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    // rtmx:req REQ-HITL-001
    #[rstest]
    #[case(ToolCall::WriteFile { path: FilePath::new_unchecked("src/main.rs"), content: String::new() }, ToolRisk::StateMutating)]
    #[case(ToolCall::ReadFile { path: FilePath::new_unchecked("src/main.rs") }, ToolRisk::ReadOnly)]
    #[case(ToolCall::RunCommand { command: "npm test".into(), timeout_secs: 60 }, ToolRisk::StateMutating)]
    #[case(ToolCall::ListDir { path: FilePath::new_unchecked(".") }, ToolRisk::ReadOnly)]
    #[case(ToolCall::Grep { pattern: "TODO".into(), path: FilePath::new_unchecked("src") }, ToolRisk::ReadOnly)]
    fn tool_risk_classification(#[case] call: ToolCall, #[case] expected: ToolRisk) {
        assert_eq!(call.risk(), expected);
    }

    // rtmx:req REQ-BUILD-001
    #[test]
    fn session_id_is_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }
}
