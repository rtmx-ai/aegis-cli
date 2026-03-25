//! Domain error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("file access denied by .aegisignore: {path}")]
    FileBlocked { path: String },

    #[error("tool execution denied by HITL gate")]
    PermissionDenied,

    #[error("requirement not found: {id}")]
    RequirementNotFound { id: String },

    #[error("provider error: {message}")]
    ProviderError { message: String },

    #[error("configuration error: {message}")]
    ConfigError { message: String },

    #[error("audit ledger error: {message}")]
    AuditError { message: String },

    #[error("{0}")]
    Other(String),
}
