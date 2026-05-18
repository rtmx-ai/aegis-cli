//! Domain value objects with compile-time safety guarantees.
//!
//! Newtypes prevent accidental misuse (e.g., passing a SessionId where a
//! RequestId is expected). Construction validates invariants.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for an agent session.
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::SessionId;
/// let id = SessionId::new();
/// // Each session ID is unique.
/// let id2 = SessionId::new();
/// assert_ne!(id, id2);
/// ```
///
/// Display produces the underlying UUID string:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::SessionId;
/// let id = SessionId::new();
/// let display = format!("{id}");
/// assert!(!display.is_empty());
/// ```
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
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::RequestId;
/// let rid = RequestId::new();
/// let rid2 = RequestId::new();
/// assert_ne!(rid, rid2);
/// ```
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
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::RequirementId;
/// let rid = RequirementId::new("REQ-BUILD-001");
/// assert_eq!(rid.as_str(), "REQ-BUILD-001");
/// assert_eq!(format!("{rid}"), "REQ-BUILD-001");
/// ```
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
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::FilePath;
/// let fp = FilePath::new_unchecked("src/main.rs");
/// assert_eq!(fp.as_path(), std::path::Path::new("src/main.rs"));
/// ```
///
/// Display renders the path:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::FilePath;
/// let fp = FilePath::new_unchecked("/tmp/file.txt");
/// assert_eq!(format!("{fp}"), "/tmp/file.txt");
/// ```
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
///
/// # Examples
///
/// Construct a `ReadFile` variant:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::{ToolCall, FilePath};
/// let tc = ToolCall::ReadFile { path: FilePath::new_unchecked("main.rs") };
/// ```
///
/// Construct a `WriteFile` variant:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::{ToolCall, FilePath};
/// let tc = ToolCall::WriteFile {
///     path: FilePath::new_unchecked("out.txt"),
///     content: "hello".to_string(),
/// };
/// ```
///
/// Construct a `RunCommand` variant:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::ToolCall;
/// let tc = ToolCall::RunCommand {
///     command: "cargo test".to_string(),
///     timeout_secs: 60,
/// };
/// ```
///
/// Construct a `ListDir` variant:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::{ToolCall, FilePath};
/// let tc = ToolCall::ListDir { path: FilePath::new_unchecked(".") };
/// ```
///
/// Construct a `Grep` variant:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::{ToolCall, FilePath};
/// let tc = ToolCall::Grep {
///     pattern: "TODO".to_string(),
///     path: FilePath::new_unchecked("src"),
/// };
/// ```
///
/// Construct an `McpTool` variant:
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::ToolCall;
/// let tc = ToolCall::McpTool {
///     qualified_name: "server/tool".to_string(),
///     arguments: serde_json::json!({"key": "value"}),
/// };
/// ```
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
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::{ToolCall, ToolRisk, FilePath};
/// let read = ToolCall::ReadFile { path: FilePath::new_unchecked("f.rs") };
/// assert_eq!(read.risk(), ToolRisk::ReadOnly);
///
/// let write = ToolCall::WriteFile {
///     path: FilePath::new_unchecked("f.rs"),
///     content: String::new(),
/// };
/// assert_eq!(write.risk(), ToolRisk::StateMutating);
/// ```
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

    /// Returns the canonical tool name for this call.
    pub fn tool_name(&self) -> &str {
        match self {
            Self::ReadFile { .. } => "read_file",
            Self::WriteFile { .. } => "write_file",
            Self::RunCommand { .. } => "run_command",
            Self::ListDir { .. } => "list_dir",
            Self::Grep { .. } => "grep",
            Self::McpTool { qualified_name, .. } => qualified_name.as_str(),
        }
    }

    /// Create a modified copy with edited arguments (REQ-HITL-017).
    ///
    /// For RunCommand: the edited string replaces the command.
    /// For McpTool: the edited string replaces the JSON arguments.
    /// For WriteFile: the edited string replaces the content.
    /// Returns None for tool types that don't support editing.
    pub fn with_edited_args(&self, edited: &str) -> Option<Self> {
        match self {
            Self::RunCommand { timeout_secs, .. } => Some(Self::RunCommand {
                command: edited.to_string(),
                timeout_secs: *timeout_secs,
            }),
            Self::McpTool { qualified_name, .. } => {
                let args = serde_json::from_str(edited)
                    .unwrap_or(serde_json::Value::String(edited.to_string()));
                Some(Self::McpTool {
                    qualified_name: qualified_name.clone(),
                    arguments: args,
                })
            }
            Self::WriteFile { path, .. } => Some(Self::WriteFile {
                path: path.clone(),
                content: edited.to_string(),
            }),
            // Read-only tools don't go through HITL
            Self::ReadFile { .. } | Self::ListDir { .. } | Self::Grep { .. } => None,
        }
    }
}

/// The result of executing a tool call.
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::ToolResult;
/// let ok = ToolResult::Success { output: "done".to_string() };
/// let err = ToolResult::Error { message: "fail".to_string() };
/// let denied = ToolResult::PermissionDenied { reason: "blocked".to_string() };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolResult {
    Success {
        output: String,
    },
    Error {
        message: String,
    },
    PermissionDenied {
        reason: String,
    },
    /// Tool was skipped by user but session continues (REQ-HITL-018).
    Skipped {
        tool_name: String,
    },
}

/// HITL approval decision.
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::types::ApprovalDecision;
/// let decision = ApprovalDecision::Approved;
/// assert_eq!(decision, ApprovalDecision::Approved);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Approved,
    Denied,
    Edited,
    Skipped,
    /// Auto-denied because the HITL timeout expired (REQ-HITL-003).
    TimedOut,
}

/// HITL approval response with optional edited arguments (REQ-HITL-017).
///
/// Wraps an `ApprovalDecision` with optional modified tool arguments
/// when the user chooses to edit before approving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalResponse {
    pub decision: ApprovalDecision,
    /// Modified tool arguments (JSON or command string) when decision is Edited.
    pub edited_args: Option<String>,
}

impl ApprovalResponse {
    pub fn simple(decision: ApprovalDecision) -> Self {
        Self {
            decision,
            edited_args: None,
        }
    }

    pub fn edited(args: String) -> Self {
        Self {
            decision: ApprovalDecision::Edited,
            edited_args: Some(args),
        }
    }
}

impl From<ApprovalDecision> for ApprovalResponse {
    fn from(decision: ApprovalDecision) -> Self {
        Self::simple(decision)
    }
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

// ---- AI Bill of Materials (REQ-LLM-050) ----

/// License type for a model family.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelLicense {
    Apache2,
    MIT,
    /// Research-only license (no commercial use).
    Research,
    Proprietary,
    #[default]
    Unknown,
}

impl fmt::Display for ModelLicense {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Apache2 => write!(f, "Apache-2.0"),
            Self::MIT => write!(f, "MIT"),
            Self::Research => write!(f, "Research-only"),
            Self::Proprietary => write!(f, "Proprietary"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Export control classification for a model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportClassification {
    /// Export Administration Regulations apply.
    Ear,
    /// International Traffic in Arms Regulations apply.
    Itar,
    /// No known export restrictions.
    #[default]
    None,
}

/// AI Bill of Materials: provenance metadata for a model family.
///
/// Captures training data lineage, weights origin, licensing, known
/// vulnerabilities, and export classification. All fields are optional
/// except `model_family`, since BOM data is often incomplete for
/// open-weight models.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiBom {
    /// Model family name (e.g., "llama3", "gemma4").
    pub model_family: String,
    /// Country of origin for the model weights.
    pub origin_country: Option<String>,
    /// License governing model use.
    #[serde(default)]
    pub license: ModelLicense,
    /// Known training data sources (e.g., "CommonCrawl", "Wikipedia").
    #[serde(default)]
    pub training_data_sources: Vec<String>,
    /// Fine-tuning chain: list of organizations that fine-tuned the model.
    #[serde(default)]
    pub fine_tune_chain: Vec<String>,
    /// Known vulnerabilities or safety concerns.
    #[serde(default)]
    pub known_vulnerabilities: Vec<String>,
    /// Export control classification.
    #[serde(default)]
    pub export_classification: ExportClassification,
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

    // rtmx:req REQ-LLM-050
    #[test]
    fn test_ai_bom_default_values() {
        let bom = AiBom::default();
        assert!(bom.model_family.is_empty());
        assert!(bom.origin_country.is_none());
        assert_eq!(bom.license, ModelLicense::Unknown);
        assert!(bom.training_data_sources.is_empty());
        assert!(bom.fine_tune_chain.is_empty());
        assert!(bom.known_vulnerabilities.is_empty());
        assert_eq!(bom.export_classification, ExportClassification::None);
    }

    // rtmx:req REQ-LLM-050
    #[test]
    fn test_ai_bom_serializes_to_json() {
        let bom = AiBom {
            model_family: "llama3".to_string(),
            origin_country: Some("US".to_string()),
            license: ModelLicense::Apache2,
            training_data_sources: vec!["CommonCrawl".to_string()],
            fine_tune_chain: vec!["Meta".to_string()],
            known_vulnerabilities: vec![],
            export_classification: ExportClassification::None,
        };
        let json = serde_json::to_string(&bom).unwrap();
        assert!(json.contains("llama3"));
        assert!(json.contains("Apache2"));
        assert!(json.contains("CommonCrawl"));
        // Deserialize round-trip
        let parsed: AiBom = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model_family, "llama3");
        assert_eq!(parsed.license, ModelLicense::Apache2);
    }

    // rtmx:req REQ-LLM-050
    #[test]
    fn test_model_license_display() {
        assert_eq!(ModelLicense::Apache2.to_string(), "Apache-2.0");
        assert_eq!(ModelLicense::MIT.to_string(), "MIT");
        assert_eq!(ModelLicense::Research.to_string(), "Research-only");
        assert_eq!(ModelLicense::Proprietary.to_string(), "Proprietary");
        assert_eq!(ModelLicense::Unknown.to_string(), "Unknown");
    }

    // rtmx:req REQ-LLM-050
    #[test]
    fn test_export_classification_default() {
        let ec = ExportClassification::default();
        assert_eq!(ec, ExportClassification::None);
    }
}
