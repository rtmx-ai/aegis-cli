//! Enterprise BYOC gateway configuration.
//!
//! When BYOC mode is detected (via [`crate::byoc::detect_byoc_environment`]),
//! this module creates the gateway configuration that routes LLM traffic
//! through a corporate proxy. Supports service token and mTLS auth methods.

use std::io;
use std::path::Path;

use crate::config::{AegisConfig, BackendConfig, Mode};

/// Authentication method for the BYOC gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    /// Bearer token authentication (via AEGIS_SERVICE_TOKEN or file).
    ServiceToken,
    /// Mutual TLS with client certificate and key.
    MtlsCertificate,
    /// No additional authentication (gateway handles auth externally).
    None,
}

/// Configuration for an enterprise BYOC gateway.
#[derive(Debug, Clone)]
pub struct ByocGatewayConfig {
    /// The gateway URL that proxies LLM requests.
    pub gateway_url: String,
    /// How the client authenticates to the gateway.
    pub auth_method: AuthMethod,
    /// Optional organization identifier for multi-tenant gateways.
    pub org_id: Option<String>,
}

/// Create an [`AegisConfig`] configured for enterprise BYOC mode.
///
/// Sets the mode to `EnterpriseByoc` and routes traffic through
/// the specified gateway URL. The model defaults to `"gateway"`
/// since the gateway handles model selection.
pub fn create_byoc_config(
    gateway_url: &str,
    auth: AuthMethod,
    model: Option<&str>,
) -> AegisConfig {
    let provider = match auth {
        AuthMethod::ServiceToken => "byoc-token",
        AuthMethod::MtlsCertificate => "byoc-mtls",
        AuthMethod::None => "byoc",
    };

    AegisConfig {
        version: "1.0".to_string(),
        mode: Mode::EnterpriseByoc,
        backend: BackendConfig {
            provider: provider.to_string(),
            model: model.unwrap_or("gateway").to_string(),
            endpoint: gateway_url.to_string(),
            region: None,
            max_tokens: 4096,
        },
        infra: Default::default(),
    }
}

/// Write the BYOC gateway configuration to a YAML file.
///
/// Serializes the gateway metadata (URL, auth method, org_id) into a
/// simple key=value file at `<config_dir>/gateway.conf`, separate from
/// the main `config.yaml`. The main config is also written via
/// [`AegisConfig::save`].
pub fn write_byoc_config(config_dir: &Path, config: &ByocGatewayConfig) -> io::Result<()> {
    std::fs::create_dir_all(config_dir)?;

    let auth_str = match config.auth_method {
        AuthMethod::ServiceToken => "service-token",
        AuthMethod::MtlsCertificate => "mtls",
        AuthMethod::None => "none",
    };

    let mut content = format!(
        "# Enterprise BYOC gateway configuration\n\
         gateway_url={}\n\
         auth_method={}\n",
        config.gateway_url, auth_str,
    );

    if let Some(ref org) = config.org_id {
        content.push_str(&format!("org_id={}\n", org));
    }

    let path = config_dir.join("gateway.conf");
    std::fs::write(&path, content)?;

    // Set permissions to 0600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

/// Read a BYOC gateway configuration from `<config_dir>/gateway.conf`.
///
/// Returns `None` if the file does not exist or cannot be parsed.
pub fn read_byoc_config(config_dir: &Path) -> Option<ByocGatewayConfig> {
    let path = config_dir.join("gateway.conf");
    let content = std::fs::read_to_string(&path).ok()?;

    let mut gateway_url = None;
    let mut auth_method = AuthMethod::None;
    let mut org_id = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(val) = trimmed.strip_prefix("gateway_url=") {
            let v = val.trim();
            if !v.is_empty() {
                gateway_url = Some(v.to_string());
            }
        } else if let Some(val) = trimmed.strip_prefix("auth_method=") {
            auth_method = match val.trim() {
                "service-token" => AuthMethod::ServiceToken,
                "mtls" => AuthMethod::MtlsCertificate,
                _ => AuthMethod::None,
            };
        } else if let Some(val) = trimmed.strip_prefix("org_id=") {
            let v = val.trim();
            if !v.is_empty() {
                org_id = Some(v.to_string());
            }
        }
    }

    gateway_url.map(|url| ByocGatewayConfig {
        gateway_url: url,
        auth_method,
        org_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // @req REQ-ONBOARD-013
    #[test]
    fn create_byoc_config_sets_enterprise_mode() {
        let config =
            create_byoc_config("https://gateway.corp.mil", AuthMethod::ServiceToken, None);
        assert_eq!(config.mode, Mode::EnterpriseByoc);
        assert_eq!(config.backend.endpoint, "https://gateway.corp.mil");
        assert_eq!(config.backend.provider, "byoc-token");
        assert_eq!(config.backend.model, "gateway");
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn create_byoc_config_with_mtls() {
        let config = create_byoc_config(
            "https://gateway.corp.mil",
            AuthMethod::MtlsCertificate,
            None,
        );
        assert_eq!(config.backend.provider, "byoc-mtls");
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn create_byoc_config_with_no_auth() {
        let config = create_byoc_config("https://gateway.corp.mil", AuthMethod::None, None);
        assert_eq!(config.backend.provider, "byoc");
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn create_byoc_config_with_custom_model() {
        let config = create_byoc_config(
            "https://gateway.corp.mil",
            AuthMethod::ServiceToken,
            Some("gemini-3.1-pro"),
        );
        assert_eq!(config.backend.model, "gemini-3.1-pro");
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn write_and_read_byoc_config_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let gw = ByocGatewayConfig {
            gateway_url: "https://gateway.example.com".to_string(),
            auth_method: AuthMethod::ServiceToken,
            org_id: Some("org-123".to_string()),
        };

        write_byoc_config(tmp.path(), &gw).unwrap();
        let loaded = read_byoc_config(tmp.path()).unwrap();

        assert_eq!(loaded.gateway_url, "https://gateway.example.com");
        assert_eq!(loaded.auth_method, AuthMethod::ServiceToken);
        assert_eq!(loaded.org_id, Some("org-123".to_string()));
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn write_and_read_byoc_config_without_org_id() {
        let tmp = TempDir::new().unwrap();
        let gw = ByocGatewayConfig {
            gateway_url: "https://gw.internal.mil".to_string(),
            auth_method: AuthMethod::MtlsCertificate,
            org_id: None,
        };

        write_byoc_config(tmp.path(), &gw).unwrap();
        let loaded = read_byoc_config(tmp.path()).unwrap();

        assert_eq!(loaded.gateway_url, "https://gw.internal.mil");
        assert_eq!(loaded.auth_method, AuthMethod::MtlsCertificate);
        assert!(loaded.org_id.is_none());
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn read_byoc_config_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        assert!(
            read_byoc_config(tmp.path()).is_none(),
            "Should return None when gateway.conf does not exist"
        );
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn write_byoc_config_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("deep").join("nested");
        let gw = ByocGatewayConfig {
            gateway_url: "https://gw.example.com".to_string(),
            auth_method: AuthMethod::None,
            org_id: None,
        };
        write_byoc_config(&nested, &gw).unwrap();
        assert!(read_byoc_config(&nested).is_some());
    }

    // @req REQ-ONBOARD-013
    #[cfg(unix)]
    #[test]
    fn write_byoc_config_sets_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let gw = ByocGatewayConfig {
            gateway_url: "https://gw.example.com".to_string(),
            auth_method: AuthMethod::ServiceToken,
            org_id: None,
        };
        write_byoc_config(tmp.path(), &gw).unwrap();

        let path = tmp.path().join("gateway.conf");
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "gateway.conf should have 0600 permissions"
        );
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn auth_method_equality() {
        assert_eq!(AuthMethod::ServiceToken, AuthMethod::ServiceToken);
        assert_eq!(AuthMethod::MtlsCertificate, AuthMethod::MtlsCertificate);
        assert_eq!(AuthMethod::None, AuthMethod::None);
        assert_ne!(AuthMethod::ServiceToken, AuthMethod::MtlsCertificate);
    }

    // @req REQ-ONBOARD-013
    #[test]
    fn read_byoc_config_ignores_comments_and_blanks() {
        let tmp = TempDir::new().unwrap();
        let content = "# header comment\n\n\
                       gateway_url=https://gw.example.com\n\
                       # auth comment\n\
                       auth_method=mtls\n\
                       org_id=my-org\n";
        std::fs::write(tmp.path().join("gateway.conf"), content).unwrap();

        let loaded = read_byoc_config(tmp.path()).unwrap();
        assert_eq!(loaded.gateway_url, "https://gw.example.com");
        assert_eq!(loaded.auth_method, AuthMethod::MtlsCertificate);
        assert_eq!(loaded.org_id, Some("my-org".to_string()));
    }
}
