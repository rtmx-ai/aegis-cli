//! Configuration management for ~/.aegis/config.yaml.
//!
//! Handles reading, writing, and validating the aegis config file.
//! Config contains routing metadata only -- never secrets.

use aegis_domain::error::DomainError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Deployment mode for aegis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Local,
    SelfServiceByoc,
    EnterpriseByoc,
    ManagedSaas,
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
        assert!(!yaml.contains("key"), "Config should not contain API keys");
        assert!(!yaml.contains("token"), "Config should not contain tokens");
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
