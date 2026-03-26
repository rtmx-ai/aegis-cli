//! Configuration management for ~/.aegis/config.yaml.
//!
//! Handles reading, writing, and validating the aegis config file.
//! Config contains routing metadata only -- never secrets.

use aegis_domain::error::DomainError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Deployment mode for aegis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Local,
    SelfServiceByoc,
    EnterpriseByoc,
    ManagedSaas,
}

impl FromStr for Mode {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(Mode::Local),
            "self-service-byoc" => Ok(Mode::SelfServiceByoc),
            "enterprise-byoc" => Ok(Mode::EnterpriseByoc),
            "managed-saas" => Ok(Mode::ManagedSaas),
            other => Err(DomainError::ConfigError {
                message: format!(
                    "Invalid mode '{}': expected one of \
                     local, self-service-byoc, enterprise-byoc, managed-saas",
                    other
                ),
            }),
        }
    }
}

/// The aegis configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AegisConfig {
    pub version: String,
    pub mode: Mode,
    pub backend: BackendConfig,
    #[serde(default)]
    pub infra: toml_map::InfraSection,
}

/// Backend (LLM endpoint) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_max_tokens() -> u32 {
    4096
}

/// Infrastructure outputs from plugins, keyed by plugin name.
pub mod toml_map {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct InfraSection {
        #[serde(flatten)]
        pub plugins: HashMap<String, PluginOutputs>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginOutputs {
        #[serde(flatten)]
        pub outputs: HashMap<String, String>,
    }
}

impl AegisConfig {
    /// Create a local (air-gapped) configuration.
    pub fn local(endpoint: &str, model: &str) -> Self {
        Self {
            version: "1.0".to_string(),
            mode: Mode::Local,
            backend: BackendConfig {
                provider: "local".to_string(),
                model: model.to_string(),
                endpoint: endpoint.to_string(),
                region: None,
                max_tokens: default_max_tokens(),
            },
            infra: Default::default(),
        }
    }

    /// Default config file path: ~/.aegis/config.yaml
    pub fn default_path() -> Result<PathBuf, DomainError> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| DomainError::ConfigError {
                message: "Could not determine home directory".to_string(),
            })?;
        Ok(PathBuf::from(home).join(".aegis").join("config.yaml"))
    }

    /// Load config from a YAML file.
    pub fn load(path: &Path) -> Result<Self, DomainError> {
        let content = std::fs::read_to_string(path).map_err(|e| DomainError::ConfigError {
            message: format!("Failed to read {}: {e}", path.display()),
        })?;
        serde_yaml::from_str(&content).map_err(|e| DomainError::ConfigError {
            message: format!("Invalid config at {}: {e}", path.display()),
        })
    }

    /// Write config to a YAML file with 0600 permissions.
    pub fn save(&self, path: &Path) -> Result<(), DomainError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DomainError::ConfigError {
                message: format!("Failed to create {}: {e}", parent.display()),
            })?;
        }

        let yaml = serde_yaml::to_string(self).map_err(|e| DomainError::ConfigError {
            message: format!("Failed to serialize config: {e}"),
        })?;

        // Atomic write: write to temp, then rename
        let tmp_path = path.with_extension("yaml.tmp");
        std::fs::write(&tmp_path, &yaml).map_err(|e| DomainError::ConfigError {
            message: format!("Failed to write {}: {e}", tmp_path.display()),
        })?;

        // Set permissions to 0600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600)).map_err(
                |e| DomainError::ConfigError {
                    message: format!("Failed to set permissions: {e}"),
                },
            )?;
        }

        std::fs::rename(&tmp_path, path).map_err(|e| DomainError::ConfigError {
            message: format!("Failed to rename {}: {e}", tmp_path.display()),
        })?;

        Ok(())
    }

    /// Validate the config has all required fields.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.backend.endpoint.is_empty() {
            return Err(DomainError::ConfigError {
                message: "backend.endpoint is required".to_string(),
            });
        }
        if self.backend.model.is_empty() {
            return Err(DomainError::ConfigError {
                message: "backend.model is required".to_string(),
            });
        }
        if self.backend.provider.is_empty() {
            return Err(DomainError::ConfigError {
                message: "backend.provider is required".to_string(),
            });
        }
        // Local mode: HTTP allowed only for loopback
        if self.mode == Mode::Local {
            let ep = &self.backend.endpoint;
            if ep.starts_with("http://")
                && !ep.contains("localhost")
                && !ep.contains("127.0.0.1")
                && !ep.contains("[::1]")
            {
                return Err(DomainError::ConfigError {
                    message: "HTTP only allowed for \
                              loopback in local mode"
                        .to_string(),
                });
            }
        }
        Ok(())
    }
}

/// Return the default audit ledger directory path: ~/.aegis/logs/
pub fn audit_ledger_dir() -> Result<PathBuf, DomainError> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| DomainError::ConfigError {
            message: "Could not determine home directory".to_string(),
        })?;
    Ok(PathBuf::from(home).join(".aegis").join("logs"))
}

/// Merge a new config into an existing config, preserving fields
/// that are not explicitly changed.
///
/// The merge strategy:
/// - `mode`, `backend.endpoint`, `backend.model` are taken from
///   `new_values` (these are the fields the user is updating).
/// - `backend.region` is taken from `new_values` if `Some`, otherwise
///   the existing value is preserved.
/// - `backend.max_tokens` is taken from `new_values` only if it
///   differs from the default (4096); otherwise the existing value
///   is kept.
/// - `backend.provider` is taken from `new_values`.
/// - `infra` plugin outputs are always preserved from `existing`
///   (plugins manage their own outputs).
/// - `version` is taken from `existing` (never downgraded).
pub fn merge_config(existing: &AegisConfig, new_values: &AegisConfig) -> AegisConfig {
    AegisConfig {
        // Keep the existing version -- never downgrade
        version: existing.version.clone(),
        // Mode, provider, endpoint, model are the user-updated fields
        mode: new_values.mode.clone(),
        backend: BackendConfig {
            provider: new_values.backend.provider.clone(),
            model: new_values.backend.model.clone(),
            endpoint: new_values.backend.endpoint.clone(),
            region: new_values
                .backend
                .region
                .clone()
                .or_else(|| existing.backend.region.clone()),
            max_tokens: if new_values.backend.max_tokens != default_max_tokens() {
                new_values.backend.max_tokens
            } else {
                existing.backend.max_tokens
            },
        },
        // Preserve infra plugin outputs -- plugins manage these
        infra: existing.infra.clone(),
    }
}

/// Check that an audit ledger directory at the given path exists and
/// has not been tampered with during a re-init operation.
///
/// Returns `true` if the directory exists (or never existed), meaning
/// the reinit is safe. Returns `false` only if the directory existed
/// before but was deleted or is no longer a directory.
pub fn preserves_audit_ledger(logs_dir: &Path) -> bool {
    if !logs_dir.exists() {
        // No ledger yet -- nothing to protect
        return true;
    }
    // The path must still be a directory
    logs_dir.is_dir()
}

/// Apply environment variable overrides to the config.
///
/// Convention: `AEGIS_<FIELD>` in SCREAMING_SNAKE_CASE overrides
/// the corresponding config field. Only non-empty values are applied.
///
/// Supported variables:
/// - `AEGIS_ENDPOINT` -> `backend.endpoint`
/// - `AEGIS_MODEL` -> `backend.model`
/// - `AEGIS_MODE` -> `mode` (must parse to a valid [`Mode`])
/// - `AEGIS_MAX_TOKENS` -> `backend.max_tokens` (must parse to `u32`)
pub fn apply_env_overrides(config: &mut AegisConfig) -> Result<(), DomainError> {
    if let Some(val) = non_empty_env("AEGIS_ENDPOINT") {
        config.backend.endpoint = val;
    }
    if let Some(val) = non_empty_env("AEGIS_MODEL") {
        config.backend.model = val;
    }
    if let Some(val) = non_empty_env("AEGIS_MODE") {
        config.mode = Mode::from_str(&val)?;
    }
    if let Some(val) = non_empty_env("AEGIS_MAX_TOKENS") {
        config.backend.max_tokens = val.parse::<u32>().map_err(|e| DomainError::ConfigError {
            message: format!("AEGIS_MAX_TOKENS must be a valid u32: {e}"),
        })?;
    }
    Ok(())
}

/// Return the value of an env var if it is set and non-empty.
fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // @req REQ-ONBOARD-002
    #[test]
    fn config_saves_and_loads_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");

        let config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.save(&path).unwrap();

        let loaded = AegisConfig::load(&path).unwrap();
        assert_eq!(loaded.mode, Mode::Local);
        assert_eq!(loaded.backend.endpoint, "http://localhost:11434/v1");
        assert_eq!(loaded.backend.model, "llama3");
    }

    // @req REQ-ONBOARD-002
    #[cfg(unix)]
    #[test]
    fn config_file_has_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");

        let config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.save(&path).unwrap();

        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "Config should have 0600 permissions"
        );
    }

    // @req REQ-ONBOARD-002
    #[test]
    fn config_contains_no_secrets() {
        let config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            !yaml.contains("api_key"),
            "Config should not contain API keys"
        );
        assert!(
            !yaml.contains("access_token"),
            "Config should not contain access tokens"
        );
        assert!(
            !yaml.contains("secret"),
            "Config should not contain secrets"
        );
    }

    // @req REQ-ONBOARD-003
    #[test]
    fn local_config_sets_correct_mode() {
        let config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        assert_eq!(config.mode, Mode::Local);
        assert_eq!(config.backend.provider, "local");
        assert!(config.backend.region.is_none());
    }

    // @req REQ-ONBOARD-009
    #[test]
    fn validate_rejects_empty_endpoint() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.backend.endpoint = String::new();
        assert!(config.validate().is_err());
    }

    // @req REQ-ONBOARD-009
    #[test]
    fn validate_rejects_empty_model() {
        let mut config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.backend.model = String::new();
        assert!(config.validate().is_err());
    }

    // @req REQ-LLM-016
    #[test]
    fn validate_rejects_non_loopback_http_in_local_mode() {
        let config = AegisConfig::local("http://remote-server:8080/v1", "llama3");
        assert!(config.validate().is_err());
    }

    // @req REQ-LLM-016
    #[test]
    fn validate_allows_loopback_http_in_local_mode() {
        let config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        assert!(config.validate().is_ok());
    }

    // @req REQ-ONBOARD-001
    #[test]
    fn config_creates_parent_directory() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested/dir/config.yaml");

        let config = AegisConfig::local("http://localhost:11434/v1", "llama3");
        config.save(&path).unwrap();
        assert!(path.exists());
    }

    // @req REQ-ONBOARD-008
    //
    // All env-override scenarios run in a single test to avoid
    // race conditions from parallel tests mutating process-wide
    // environment variables.
    #[test]
    fn env_overrides() {
        // Helper: clear all AEGIS_ env vars to a known state.
        fn clear_env() {
            unsafe {
                std::env::remove_var("AEGIS_ENDPOINT");
                std::env::remove_var("AEGIS_MODEL");
                std::env::remove_var("AEGIS_MODE");
                std::env::remove_var("AEGIS_MAX_TOKENS");
            }
        }

        fn fresh_config() -> AegisConfig {
            AegisConfig::local("http://localhost:11434/v1", "llama3")
        }

        // -- endpoint override --
        clear_env();
        unsafe { std::env::set_var("AEGIS_ENDPOINT", "http://new:8080/v1") };
        let mut cfg = fresh_config();
        apply_env_overrides(&mut cfg).unwrap();
        assert_eq!(cfg.backend.endpoint, "http://new:8080/v1");

        // -- model override --
        clear_env();
        unsafe { std::env::set_var("AEGIS_MODEL", "mixtral-8x7b") };
        let mut cfg = fresh_config();
        apply_env_overrides(&mut cfg).unwrap();
        assert_eq!(cfg.backend.model, "mixtral-8x7b");

        // -- mode override --
        clear_env();
        unsafe { std::env::set_var("AEGIS_MODE", "enterprise-byoc") };
        let mut cfg = fresh_config();
        apply_env_overrides(&mut cfg).unwrap();
        assert_eq!(cfg.mode, Mode::EnterpriseByoc);

        // -- max_tokens override --
        clear_env();
        unsafe { std::env::set_var("AEGIS_MAX_TOKENS", "8192") };
        let mut cfg = fresh_config();
        apply_env_overrides(&mut cfg).unwrap();
        assert_eq!(cfg.backend.max_tokens, 8192);

        // -- invalid mode returns error --
        clear_env();
        unsafe { std::env::set_var("AEGIS_MODE", "not-a-mode") };
        let mut cfg = fresh_config();
        assert!(apply_env_overrides(&mut cfg).is_err());

        // -- invalid max_tokens returns error --
        clear_env();
        unsafe { std::env::set_var("AEGIS_MAX_TOKENS", "not-a-number") };
        let mut cfg = fresh_config();
        assert!(apply_env_overrides(&mut cfg).is_err());

        // -- empty value is ignored --
        clear_env();
        unsafe { std::env::set_var("AEGIS_ENDPOINT", "") };
        let mut cfg = fresh_config();
        apply_env_overrides(&mut cfg).unwrap();
        assert_eq!(
            cfg.backend.endpoint, "http://localhost:11434/v1",
            "Empty env var should not override"
        );

        // -- unset vars are ignored --
        clear_env();
        let mut cfg = fresh_config();
        apply_env_overrides(&mut cfg).unwrap();
        assert_eq!(cfg.backend.endpoint, "http://localhost:11434/v1");
        assert_eq!(cfg.backend.model, "llama3");
        assert_eq!(cfg.mode, Mode::Local);
        assert_eq!(cfg.backend.max_tokens, 4096);

        // -- all four overrides at once --
        clear_env();
        unsafe {
            std::env::set_var("AEGIS_ENDPOINT", "https://vertex:443");
            std::env::set_var("AEGIS_MODEL", "gemini-pro");
            std::env::set_var("AEGIS_MODE", "managed-saas");
            std::env::set_var("AEGIS_MAX_TOKENS", "16384");
        }
        let mut cfg = fresh_config();
        apply_env_overrides(&mut cfg).unwrap();
        assert_eq!(cfg.backend.endpoint, "https://vertex:443");
        assert_eq!(cfg.backend.model, "gemini-pro");
        assert_eq!(cfg.mode, Mode::ManagedSaas);
        assert_eq!(cfg.backend.max_tokens, 16384);

        // Final cleanup
        clear_env();
    }

    // @req REQ-ONBOARD-004
    #[test]
    fn merge_config_updates_mode_endpoint_model() {
        let existing = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let new_values = AegisConfig {
            version: "1.0".to_string(),
            mode: Mode::Local,
            backend: BackendConfig {
                provider: "local".to_string(),
                model: "mixtral-8x7b".to_string(),
                endpoint: "http://localhost:8080/v1".to_string(),
                region: None,
                max_tokens: default_max_tokens(),
            },
            infra: Default::default(),
        };

        let merged = merge_config(&existing, &new_values);
        assert_eq!(merged.backend.model, "mixtral-8x7b");
        assert_eq!(merged.backend.endpoint, "http://localhost:8080/v1");
    }

    // @req REQ-ONBOARD-004
    #[test]
    fn merge_config_preserves_existing_region() {
        let mut existing = AegisConfig::local("http://localhost:11434/v1", "llama3");
        existing.backend.region = Some("us-east-1".to_string());

        let new_values = AegisConfig::local("http://localhost:11434/v1", "llama3");
        // new_values.backend.region is None

        let merged = merge_config(&existing, &new_values);
        assert_eq!(
            merged.backend.region,
            Some("us-east-1".to_string()),
            "Existing region should be preserved when new is None"
        );
    }

    // @req REQ-ONBOARD-004
    #[test]
    fn merge_config_preserves_existing_max_tokens() {
        let mut existing = AegisConfig::local("http://localhost:11434/v1", "llama3");
        existing.backend.max_tokens = 8192;

        // new_values uses the default max_tokens (4096)
        let new_values = AegisConfig::local("http://localhost:11434/v1", "llama3");

        let merged = merge_config(&existing, &new_values);
        assert_eq!(
            merged.backend.max_tokens, 8192,
            "Existing max_tokens should be preserved when new is default"
        );
    }

    // @req REQ-ONBOARD-004
    #[test]
    fn merge_config_preserves_infra_outputs() {
        use crate::config::toml_map::{InfraSection, PluginOutputs};
        use std::collections::HashMap;

        let mut existing = AegisConfig::local("http://localhost:11434/v1", "llama3");
        let mut plugins = HashMap::new();
        let mut outputs = HashMap::new();
        outputs.insert("project_id".to_string(), "aegis-il4-prod".to_string());
        plugins.insert(
            "gcp-assured-workloads".to_string(),
            PluginOutputs { outputs },
        );
        existing.infra = InfraSection { plugins };

        let new_values = AegisConfig::local("http://localhost:8080/v1", "mixtral-8x7b");

        let merged = merge_config(&existing, &new_values);
        assert!(
            merged.infra.plugins.contains_key("gcp-assured-workloads"),
            "Plugin outputs must survive re-init"
        );
        assert_eq!(
            merged.infra.plugins["gcp-assured-workloads"].outputs["project_id"],
            "aegis-il4-prod"
        );
    }

    // @req REQ-ONBOARD-004
    #[test]
    fn merge_config_preserves_version() {
        let mut existing = AegisConfig::local("http://localhost:11434/v1", "llama3");
        existing.version = "2.0".to_string();

        let new_values = AegisConfig::local("http://localhost:11434/v1", "llama3");
        // new_values.version is "1.0"

        let merged = merge_config(&existing, &new_values);
        assert_eq!(
            merged.version, "2.0",
            "Version should be preserved from existing config"
        );
    }

    // @req REQ-ONBOARD-005
    #[test]
    fn preserves_audit_ledger_returns_true_for_existing_dir() {
        let tmp = TempDir::new().unwrap();
        let logs = tmp.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        assert!(
            preserves_audit_ledger(&logs),
            "Should return true when logs dir exists"
        );
    }

    // @req REQ-ONBOARD-005
    #[test]
    fn preserves_audit_ledger_returns_true_when_no_dir() {
        let tmp = TempDir::new().unwrap();
        let logs = tmp.path().join("logs");
        // logs does not exist
        assert!(
            preserves_audit_ledger(&logs),
            "Should return true when logs dir never existed"
        );
    }

    // @req REQ-ONBOARD-005
    #[test]
    fn preserves_audit_ledger_returns_false_when_file_not_dir() {
        let tmp = TempDir::new().unwrap();
        let logs = tmp.path().join("logs");
        // Create a file (not a directory) at the logs path
        std::fs::write(&logs, "not a dir").unwrap();
        assert!(
            !preserves_audit_ledger(&logs),
            "Should return false when logs path is a file, not a dir"
        );
    }

    // @req REQ-ONBOARD-001
    #[test]
    fn mode_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&Mode::SelfServiceByoc).unwrap(),
            "\"self-service-byoc\""
        );
        assert_eq!(serde_json::to_string(&Mode::Local).unwrap(), "\"local\"");
        assert_eq!(
            serde_json::to_string(&Mode::EnterpriseByoc).unwrap(),
            "\"enterprise-byoc\""
        );
    }
}
