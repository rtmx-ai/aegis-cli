//! Credential rotation without re-provisioning infrastructure.
//!
//! Allows rotating API keys, service account paths, and endpoints
//! without running `aegis init` again. Validates new credentials
//! before committing and preserves all other config fields.

use aegis_domain::error::DomainError;
use std::path::PathBuf;

use crate::config::AegisConfig;

/// A partial update containing only credential-related fields.
///
/// Each field is `Option`: only `Some` values are applied during
/// rotation. All other config fields are preserved unchanged.
#[derive(Debug, Clone, Default)]
pub struct CredentialUpdate {
    /// New API key (provider-specific).
    pub api_key: Option<String>,
    /// Path to a service account JSON or similar credential file.
    pub credentials_path: Option<PathBuf>,
    /// New LLM endpoint URL.
    pub endpoint: Option<String>,
}

/// Returns `true` if the update contains at least one field to change.
pub fn has_changes(update: &CredentialUpdate) -> bool {
    update.api_key.is_some() || update.credentials_path.is_some() || update.endpoint.is_some()
}

/// Validate credential format before committing.
///
/// Checks:
/// - Non-empty / non-whitespace-only strings for `api_key` and `endpoint`
/// - If `credentials_path` is `Some`, the path must exist on disk
///
/// This is a format check only -- it does not perform live auth probes.
pub fn validate_credential_format(update: &CredentialUpdate) -> Result<(), DomainError> {
    if let Some(ref key) = update.api_key
        && key.trim().is_empty()
    {
        return Err(DomainError::ConfigError {
            message: "api_key must not be empty or whitespace-only".to_string(),
        });
    }

    if let Some(ref path) = update.credentials_path
        && !path.exists()
    {
        return Err(DomainError::ConfigError {
            message: format!("credentials_path does not exist: {}", path.display()),
        });
    }

    if let Some(ref ep) = update.endpoint
        && ep.trim().is_empty()
    {
        return Err(DomainError::ConfigError {
            message: "endpoint must not be empty or whitespace-only".to_string(),
        });
    }

    Ok(())
}

/// Apply a credential update to an existing config.
///
/// Only fields that are `Some` in the update are changed; all other
/// config fields (mode, model, region, max_tokens, infra outputs,
/// version) are preserved. Validates format before applying.
///
/// Returns an error if:
/// - The update has no changes ([`has_changes`] returns false)
/// - Format validation fails ([`validate_credential_format`])
pub fn rotate_credentials(
    config: &mut AegisConfig,
    update: &CredentialUpdate,
) -> Result<(), DomainError> {
    if !has_changes(update) {
        return Err(DomainError::ConfigError {
            message: "credential update contains no changes".to_string(),
        });
    }

    validate_credential_format(update)?;

    // Apply only the fields that are present.
    // api_key and credentials_path are credential metadata that the
    // caller manages externally (env vars, secret stores). We store
    // the endpoint in the config; the others are noted here for
    // future expansion when AegisConfig gains those fields.
    if let Some(ref ep) = update.endpoint {
        config.backend.endpoint = ep.clone();
    }

    // api_key and credentials_path are not stored in AegisConfig
    // today (secrets never land in config.yaml). These fields exist
    // in CredentialUpdate so callers can validate them via
    // validate_credential_format before passing them to an external
    // secret store or environment variable.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use tempfile::TempDir;

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn has_changes_false_when_all_none() {
        let update = CredentialUpdate::default();
        assert!(
            !has_changes(&update),
            "Default update should have no changes"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn has_changes_true_with_api_key() {
        let update = CredentialUpdate {
            api_key: Some("sk-test-123".to_string()),
            ..Default::default()
        };
        assert!(has_changes(&update));
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn has_changes_true_with_credentials_path() {
        let update = CredentialUpdate {
            credentials_path: Some(PathBuf::from("/tmp/sa.json")),
            ..Default::default()
        };
        assert!(has_changes(&update));
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn has_changes_true_with_endpoint() {
        let update = CredentialUpdate {
            endpoint: Some("https://new-endpoint:443".to_string()),
            ..Default::default()
        };
        assert!(has_changes(&update));
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_rejects_empty_api_key() {
        let update = CredentialUpdate {
            api_key: Some("".to_string()),
            ..Default::default()
        };
        let err = validate_credential_format(&update).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("api_key"),
            "Error should mention api_key: {msg}"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_rejects_whitespace_only_api_key() {
        let update = CredentialUpdate {
            api_key: Some("   \t  ".to_string()),
            ..Default::default()
        };
        assert!(validate_credential_format(&update).is_err());
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_rejects_empty_endpoint() {
        let update = CredentialUpdate {
            endpoint: Some("".to_string()),
            ..Default::default()
        };
        let err = validate_credential_format(&update).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("endpoint"),
            "Error should mention endpoint: {msg}"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_rejects_whitespace_only_endpoint() {
        let update = CredentialUpdate {
            endpoint: Some("  \n  ".to_string()),
            ..Default::default()
        };
        assert!(validate_credential_format(&update).is_err());
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_rejects_nonexistent_credentials_path() {
        let update = CredentialUpdate {
            credentials_path: Some(PathBuf::from("/nonexistent/path/to/sa.json")),
            ..Default::default()
        };
        let err = validate_credential_format(&update).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("credentials_path"),
            "Error should mention credentials_path: {msg}"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_accepts_existing_credentials_path() {
        let tmp = TempDir::new().unwrap();
        let sa_path = tmp.path().join("sa.json");
        std::fs::write(&sa_path, r#"{"type":"service_account"}"#).unwrap();

        let update = CredentialUpdate {
            credentials_path: Some(sa_path),
            ..Default::default()
        };
        assert!(validate_credential_format(&update).is_ok());
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_accepts_valid_api_key() {
        let update = CredentialUpdate {
            api_key: Some("sk-live-abc123".to_string()),
            ..Default::default()
        };
        assert!(validate_credential_format(&update).is_ok());
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn validate_accepts_none_fields() {
        // All None -- nothing to validate, passes trivially.
        // (rotate_credentials will reject this, but validate alone is fine.)
        let update = CredentialUpdate::default();
        assert!(validate_credential_format(&update).is_ok());
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_rejects_empty_update() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let update = CredentialUpdate::default();
        let err = rotate_credentials(&mut config, &update).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("no changes"),
            "Should reject empty update: {msg}"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_updates_endpoint_only() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let update = CredentialUpdate {
            endpoint: Some("http://localhost:8080/v1".to_string()),
            ..Default::default()
        };

        rotate_credentials(&mut config, &update).unwrap();
        assert_eq!(config.backend.endpoint, "http://localhost:8080/v1");
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_preserves_model_and_mode() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.backend.model = "mixtral-8x7b".to_string();

        let update = CredentialUpdate {
            endpoint: Some("http://localhost:9090/v1".to_string()),
            ..Default::default()
        };

        rotate_credentials(&mut config, &update).unwrap();
        assert_eq!(
            config.backend.model, "mixtral-8x7b",
            "Model should be preserved"
        );
        assert_eq!(
            config.mode,
            crate::config::Mode::Local,
            "Mode should be preserved"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_preserves_infra_outputs() {
        use crate::config::toml_map::{InfraSection, PluginOutputs};
        use std::collections::HashMap;

        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let mut plugins = HashMap::new();
        let mut outputs = HashMap::new();
        outputs.insert("project_id".to_string(), "aegis-il4".to_string());
        plugins.insert(
            "gcp-assured-workloads".to_string(),
            PluginOutputs { outputs },
        );
        config.infra = InfraSection { plugins };

        let update = CredentialUpdate {
            endpoint: Some("http://localhost:9090/v1".to_string()),
            ..Default::default()
        };

        rotate_credentials(&mut config, &update).unwrap();
        assert!(
            config.infra.plugins.contains_key("gcp-assured-workloads"),
            "Infra outputs must survive credential rotation"
        );
        assert_eq!(
            config.infra.plugins["gcp-assured-workloads"].outputs["project_id"],
            "aegis-il4"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_preserves_version_and_max_tokens() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.version = "2.0".to_string();
        config.backend.max_tokens = 8192;

        let update = CredentialUpdate {
            api_key: Some("new-key".to_string()),
            ..Default::default()
        };

        rotate_credentials(&mut config, &update).unwrap();
        assert_eq!(config.version, "2.0", "Version should be preserved");
        assert_eq!(
            config.backend.max_tokens, 8192,
            "max_tokens should be preserved"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_rejects_invalid_credentials() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let update = CredentialUpdate {
            endpoint: Some("  ".to_string()),
            ..Default::default()
        };

        assert!(
            rotate_credentials(&mut config, &update).is_err(),
            "Should reject whitespace-only endpoint"
        );
        // Config should not have been mutated.
        assert_eq!(
            config.backend.endpoint, "http://localhost:11434/v1",
            "Config should be unchanged after validation failure"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_with_api_key_only_preserves_endpoint() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let update = CredentialUpdate {
            api_key: Some("new-api-key".to_string()),
            ..Default::default()
        };

        rotate_credentials(&mut config, &update).unwrap();
        assert_eq!(
            config.backend.endpoint, "http://localhost:11434/v1",
            "Endpoint should be unchanged when only api_key is rotated"
        );
    }

    // rtmx:req REQ-ONBOARD-006
    #[test]
    fn rotate_with_all_fields() {
        let tmp = TempDir::new().unwrap();
        let sa_path = tmp.path().join("new-sa.json");
        std::fs::write(&sa_path, r#"{"type":"service_account"}"#).unwrap();

        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let update = CredentialUpdate {
            api_key: Some("new-key-456".to_string()),
            credentials_path: Some(sa_path),
            endpoint: Some("https://vertex.googleapis.com".to_string()),
        };

        rotate_credentials(&mut config, &update).unwrap();
        assert_eq!(config.backend.endpoint, "https://vertex.googleapis.com");
        // model, mode, version all preserved
        assert_eq!(config.backend.model, "llama3");
        assert_eq!(config.mode, crate::config::Mode::Local);
    }
}
