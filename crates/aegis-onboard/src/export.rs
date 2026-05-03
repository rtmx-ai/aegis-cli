//! Config export and import (REQ-ONBOARD-016).
//!
//! Export produces a shareable YAML template with secrets stripped.
//! Import applies a template then notes missing secrets.

use crate::config::AegisConfig;

/// Export a config, stripping sensitive fields.
///
/// Strips: `ca_bundle_path`, any credential-like fields.
/// Preserves: mode, backend (endpoint, model, provider, region),
/// schema_version, profiles, feedback defaults.
pub fn export_config(config: &AegisConfig) -> AegisConfig {
    let mut exported = config.clone();
    // Strip sensitive fields.
    exported.ca_bundle_path = None;
    // Clear infra plugin outputs (may contain project IDs / secrets).
    exported.infra = Default::default();
    exported
}

/// Import a template config into an existing config.
///
/// Overwrites: mode, backend settings (provider, model, endpoint, region),
/// profiles, schema_version.
/// Preserves: ca_bundle_path, infra outputs, feedback state from existing.
pub fn import_config(template: &AegisConfig, existing: &AegisConfig) -> AegisConfig {
    let mut merged = template.clone();
    // Preserve secrets and state from existing.
    merged.ca_bundle_path = existing.ca_bundle_path.clone();
    merged.infra = existing.infra.clone();
    merged.feedback = existing.feedback.clone();
    merged
}

/// Export config to a YAML string.
pub fn export_to_yaml(config: &AegisConfig) -> Result<String, String> {
    let exported = export_config(config);
    serde_yaml::to_string(&exported).map_err(|e| format!("Failed to serialize config: {e}"))
}

/// Import config from a YAML string, merging with existing config.
pub fn import_from_yaml(yaml: &str, existing: &AegisConfig) -> Result<AegisConfig, String> {
    let template: AegisConfig =
        serde_yaml::from_str(yaml).map_err(|e| format!("Failed to parse YAML: {e}"))?;
    Ok(import_config(&template, existing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AegisConfig, Mode, Profile};

    fn sample_config() -> AegisConfig {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.ca_bundle_path = Some("/etc/ssl/corp-ca.pem".to_string());
        config
    }

    // rtmx:req REQ-ONBOARD-016
    #[test]
    fn test_config_export_no_secrets() {
        let config = sample_config();
        let exported = export_config(&config);
        assert!(
            exported.ca_bundle_path.is_none(),
            "ca_bundle_path should be stripped"
        );
        assert!(
            exported.infra.plugins.is_empty(),
            "infra outputs should be stripped"
        );
    }

    // rtmx:req REQ-ONBOARD-016
    #[test]
    fn test_config_export_preserves_profiles() {
        let mut config = sample_config();
        let profile = Profile {
            mode: Some("local".to_string()),
            endpoint: Some("http://localhost:11434/v1".to_string()),
            region: None,
            model: Some("llama3".to_string()),
            provider: Some("local".to_string()),
        };
        config.profiles.insert("work".to_string(), profile);

        let exported = export_config(&config);
        assert!(
            exported.profiles.contains_key("work"),
            "profiles should survive export"
        );
    }

    // rtmx:req REQ-ONBOARD-016
    #[test]
    fn test_config_import_merges_settings() {
        let existing = sample_config();

        let mut template = AegisConfig::local("https://vertex:443/v1", "gemini-pro");
        template.mode = Mode::EnterpriseByoc;
        template.ca_bundle_path = None; // Template has no secrets.

        let merged = import_config(&template, &existing);
        // Template values applied.
        assert_eq!(merged.mode, Mode::EnterpriseByoc);
        assert_eq!(merged.backend.endpoint, "https://vertex:443/v1");
        // Existing secrets preserved.
        assert_eq!(
            merged.ca_bundle_path,
            Some("/etc/ssl/corp-ca.pem".to_string())
        );
    }

    // rtmx:req REQ-ONBOARD-016
    #[test]
    fn test_config_import_from_yaml() {
        let existing = sample_config();
        let yaml = r#"
schema_version: 2
version: "1.0"
mode: managed-saas
backend:
  provider: vertex-ai
  model: gemini-pro
  endpoint: https://vertex:443/v1
  max_tokens: 4096
"#;
        let merged = import_from_yaml(yaml, &existing).unwrap();
        assert_eq!(merged.mode, Mode::ManagedSaas);
        assert_eq!(
            merged.ca_bundle_path,
            Some("/etc/ssl/corp-ca.pem".to_string()),
            "existing secrets should be preserved"
        );
    }

    // rtmx:req REQ-ONBOARD-016
    #[test]
    fn test_export_to_yaml_is_valid() {
        let config = sample_config();
        let yaml = export_to_yaml(&config).unwrap();
        let parsed: AegisConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.mode, Mode::Local);
        assert!(parsed.ca_bundle_path.is_none());
    }
}
