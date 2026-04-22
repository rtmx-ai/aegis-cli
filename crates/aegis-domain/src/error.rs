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

    /// Authentication token has expired. The agent loop should attempt
    /// to refresh credentials and retry the current turn.
    #[error("authentication token expired for {provider_kind}")]
    AuthExpired { provider_kind: String },

    #[error("configuration error: {message}")]
    ConfigError { message: String },

    #[error("audit ledger error: {message}")]
    AuditError { message: String },

    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-AGENT-044
    #[test]
    fn test_auth_expired_display() {
        let err = DomainError::AuthExpired {
            provider_kind: "vertex".to_string(),
        };
        assert_eq!(err.to_string(), "authentication token expired for vertex");
    }
}
