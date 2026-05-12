//! Domain error types.

use thiserror::Error;

/// Domain-level error variants.
///
/// # Examples
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::error::DomainError;
///
/// let e = DomainError::FileBlocked { path: "/etc/shadow".to_string() };
/// assert!(e.to_string().contains("/etc/shadow"));
/// ```
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::error::DomainError;
///
/// let e = DomainError::PermissionDenied;
/// assert_eq!(e.to_string(), "tool execution denied by HITL gate");
/// ```
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::error::DomainError;
///
/// let e = DomainError::RequirementNotFound { id: "REQ-FAKE-999".to_string() };
/// assert!(e.to_string().contains("REQ-FAKE-999"));
/// ```
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::error::DomainError;
///
/// let e = DomainError::ProviderError { message: "timeout".to_string() };
/// assert!(e.to_string().contains("timeout"));
/// ```
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::error::DomainError;
///
/// let e = DomainError::AuthExpired { provider_kind: "vertex".to_string() };
/// assert!(e.to_string().contains("vertex"));
/// ```
///
/// ```
/// // rtmx:req REQ-TEST-047
/// use aegis_domain::error::DomainError;
///
/// let e = DomainError::Other("custom error".to_string());
/// assert_eq!(e.to_string(), "custom error");
/// ```
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

    /// DLP transmission gate blocked outbound content (REQ-SECURITY-006).
    #[error("DLP blocked: {reason}")]
    DlpBlocked { reason: String },

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
