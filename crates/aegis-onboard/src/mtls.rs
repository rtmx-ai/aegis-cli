//! Enterprise mTLS certificate authentication for BYOC mode.
//!
//! Supports mutual TLS by detecting client certificate and key files:
//! 1. Default paths: `~/.aegis/client.pem` and `~/.aegis/client-key.pem`
//! 2. Environment variables: `AEGIS_CLIENT_CERT` and `AEGIS_CLIENT_KEY`
//!
//! Validates that cert and key files exist and are readable before
//! storing the paths in config.

use std::fmt;
use std::path::{Path, PathBuf};

/// Default client certificate filename within the config directory.
const DEFAULT_CERT_FILENAME: &str = "client.pem";

/// Default client key filename within the config directory.
const DEFAULT_KEY_FILENAME: &str = "client-key.pem";

/// mTLS certificate and key paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlsConfig {
    /// Path to the client certificate (PEM format).
    pub cert_path: PathBuf,
    /// Path to the client private key (PEM format).
    pub key_path: PathBuf,
}

/// Errors that can occur during mTLS configuration.
#[derive(Debug)]
pub enum MtlsError {
    /// The certificate file does not exist or is not readable.
    CertNotFound(PathBuf),
    /// The key file does not exist or is not readable.
    KeyNotFound(PathBuf),
    /// The certificate file is empty.
    CertEmpty(PathBuf),
    /// The key file is empty.
    KeyEmpty(PathBuf),
}

impl fmt::Display for MtlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MtlsError::CertNotFound(p) => {
                write!(f, "Client certificate not found: {}", p.display())
            }
            MtlsError::KeyNotFound(p) => {
                write!(f, "Client key not found: {}", p.display())
            }
            MtlsError::CertEmpty(p) => {
                write!(f, "Client certificate is empty: {}", p.display())
            }
            MtlsError::KeyEmpty(p) => {
                write!(f, "Client key is empty: {}", p.display())
            }
        }
    }
}

impl std::error::Error for MtlsError {}

/// Detect mTLS certificates at default paths within the config directory.
///
/// Looks for `client.pem` and `client-key.pem` in `config_dir`.
/// Returns `Some(MtlsConfig)` only if BOTH files exist.
pub fn detect_mtls_certs(config_dir: &Path) -> Option<MtlsConfig> {
    let cert_path = config_dir.join(DEFAULT_CERT_FILENAME);
    let key_path = config_dir.join(DEFAULT_KEY_FILENAME);

    if cert_path.exists() && key_path.exists() {
        Some(MtlsConfig {
            cert_path,
            key_path,
        })
    } else {
        None
    }
}

/// Detect mTLS certificates from environment variables.
///
/// Reads `AEGIS_CLIENT_CERT` and `AEGIS_CLIENT_KEY`. Returns
/// `Some(MtlsConfig)` only if BOTH variables are set and non-empty.
pub fn detect_mtls_from_env() -> Option<MtlsConfig> {
    let cert = non_empty_env("AEGIS_CLIENT_CERT")?;
    let key = non_empty_env("AEGIS_CLIENT_KEY")?;

    Some(MtlsConfig {
        cert_path: PathBuf::from(cert),
        key_path: PathBuf::from(key),
    })
}

/// Validate that an mTLS config points to existing, non-empty files.
///
/// Checks:
/// 1. Certificate file exists and is readable
/// 2. Key file exists and is readable
/// 3. Neither file is empty
pub fn validate_mtls_config(config: &MtlsConfig) -> Result<(), MtlsError> {
    // Check cert exists
    if !config.cert_path.exists() {
        return Err(MtlsError::CertNotFound(config.cert_path.clone()));
    }

    // Check key exists
    if !config.key_path.exists() {
        return Err(MtlsError::KeyNotFound(config.key_path.clone()));
    }

    // Check cert is non-empty
    let cert_meta = std::fs::metadata(&config.cert_path)
        .map_err(|_| MtlsError::CertNotFound(config.cert_path.clone()))?;
    if cert_meta.len() == 0 {
        return Err(MtlsError::CertEmpty(config.cert_path.clone()));
    }

    // Check key is non-empty
    let key_meta = std::fs::metadata(&config.key_path)
        .map_err(|_| MtlsError::KeyNotFound(config.key_path.clone()))?;
    if key_meta.len() == 0 {
        return Err(MtlsError::KeyEmpty(config.key_path.clone()));
    }

    Ok(())
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // @req REQ-ONBOARD-018
    #[test]
    fn detect_mtls_certs_returns_some_when_both_exist() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("client.pem"),
            "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("client-key.pem"),
            "-----BEGIN PRIVATE KEY-----\ntest\n-----END PRIVATE KEY-----\n",
        )
        .unwrap();

        let result = detect_mtls_certs(tmp.path());
        assert!(result.is_some(), "Should detect when both files exist");
        let config = result.unwrap();
        assert_eq!(config.cert_path, tmp.path().join("client.pem"));
        assert_eq!(config.key_path, tmp.path().join("client-key.pem"));
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn detect_mtls_certs_returns_none_when_cert_missing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("client-key.pem"), "key content").unwrap();

        let result = detect_mtls_certs(tmp.path());
        assert!(result.is_none(), "Should return None when cert is missing");
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn detect_mtls_certs_returns_none_when_key_missing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("client.pem"), "cert content").unwrap();

        let result = detect_mtls_certs(tmp.path());
        assert!(result.is_none(), "Should return None when key is missing");
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn detect_mtls_certs_returns_none_when_neither_exists() {
        let tmp = TempDir::new().unwrap();
        let result = detect_mtls_certs(tmp.path());
        assert!(
            result.is_none(),
            "Should return None when neither file exists"
        );
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn validate_mtls_config_accepts_valid_files() {
        let tmp = TempDir::new().unwrap();
        let cert = tmp.path().join("client.pem");
        let key = tmp.path().join("client-key.pem");
        std::fs::write(&cert, "cert-content").unwrap();
        std::fs::write(&key, "key-content").unwrap();

        let config = MtlsConfig {
            cert_path: cert,
            key_path: key,
        };
        assert!(validate_mtls_config(&config).is_ok());
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn validate_mtls_config_rejects_missing_cert() {
        let tmp = TempDir::new().unwrap();
        let key = tmp.path().join("client-key.pem");
        std::fs::write(&key, "key-content").unwrap();

        let config = MtlsConfig {
            cert_path: tmp.path().join("nonexistent.pem"),
            key_path: key,
        };
        let err = validate_mtls_config(&config).unwrap_err();
        assert!(
            matches!(err, MtlsError::CertNotFound(_)),
            "Should be CertNotFound, got: {err}"
        );
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn validate_mtls_config_rejects_missing_key() {
        let tmp = TempDir::new().unwrap();
        let cert = tmp.path().join("client.pem");
        std::fs::write(&cert, "cert-content").unwrap();

        let config = MtlsConfig {
            cert_path: cert,
            key_path: tmp.path().join("nonexistent-key.pem"),
        };
        let err = validate_mtls_config(&config).unwrap_err();
        assert!(
            matches!(err, MtlsError::KeyNotFound(_)),
            "Should be KeyNotFound, got: {err}"
        );
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn validate_mtls_config_rejects_empty_cert() {
        let tmp = TempDir::new().unwrap();
        let cert = tmp.path().join("client.pem");
        let key = tmp.path().join("client-key.pem");
        std::fs::write(&cert, "").unwrap();
        std::fs::write(&key, "key-content").unwrap();

        let config = MtlsConfig {
            cert_path: cert,
            key_path: key,
        };
        let err = validate_mtls_config(&config).unwrap_err();
        assert!(
            matches!(err, MtlsError::CertEmpty(_)),
            "Should be CertEmpty, got: {err}"
        );
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn validate_mtls_config_rejects_empty_key() {
        let tmp = TempDir::new().unwrap();
        let cert = tmp.path().join("client.pem");
        let key = tmp.path().join("client-key.pem");
        std::fs::write(&cert, "cert-content").unwrap();
        std::fs::write(&key, "").unwrap();

        let config = MtlsConfig {
            cert_path: cert,
            key_path: key,
        };
        let err = validate_mtls_config(&config).unwrap_err();
        assert!(
            matches!(err, MtlsError::KeyEmpty(_)),
            "Should be KeyEmpty, got: {err}"
        );
    }

    // @req REQ-ONBOARD-018
    //
    // All env-var scenarios run in a single test to avoid race
    // conditions from parallel tests mutating process-wide env vars.
    #[test]
    fn detect_mtls_from_env_scenarios() {
        fn clear() {
            unsafe {
                std::env::remove_var("AEGIS_CLIENT_CERT");
                std::env::remove_var("AEGIS_CLIENT_KEY");
            }
        }

        // -- returns None when neither var set --
        clear();
        assert!(
            detect_mtls_from_env().is_none(),
            "Should be None when env vars unset"
        );

        // -- returns None when only cert set --
        clear();
        unsafe {
            std::env::set_var("AEGIS_CLIENT_CERT", "/path/to/cert.pem");
        }
        assert!(
            detect_mtls_from_env().is_none(),
            "Should be None when only cert var set"
        );

        // -- returns None when only key set --
        clear();
        unsafe {
            std::env::set_var("AEGIS_CLIENT_KEY", "/path/to/key.pem");
        }
        assert!(
            detect_mtls_from_env().is_none(),
            "Should be None when only key var set"
        );

        // -- returns Some when both set --
        clear();
        unsafe {
            std::env::set_var("AEGIS_CLIENT_CERT", "/etc/aegis/client.pem");
            std::env::set_var("AEGIS_CLIENT_KEY", "/etc/aegis/client-key.pem");
        }
        let result = detect_mtls_from_env();
        assert!(result.is_some(), "Should detect when both vars set");
        let config = result.unwrap();
        assert_eq!(config.cert_path, PathBuf::from("/etc/aegis/client.pem"));
        assert_eq!(config.key_path, PathBuf::from("/etc/aegis/client-key.pem"));

        // -- ignores empty cert var --
        clear();
        unsafe {
            std::env::set_var("AEGIS_CLIENT_CERT", "");
            std::env::set_var("AEGIS_CLIENT_KEY", "/path/to/key.pem");
        }
        assert!(
            detect_mtls_from_env().is_none(),
            "Should be None when cert var is empty"
        );

        // -- ignores empty key var --
        clear();
        unsafe {
            std::env::set_var("AEGIS_CLIENT_CERT", "/path/to/cert.pem");
            std::env::set_var("AEGIS_CLIENT_KEY", "");
        }
        assert!(
            detect_mtls_from_env().is_none(),
            "Should be None when key var is empty"
        );

        // Final cleanup
        clear();
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn mtls_error_display_messages() {
        let cert_err = MtlsError::CertNotFound(PathBuf::from("/a/cert.pem"));
        assert!(
            cert_err.to_string().contains("certificate not found"),
            "Display: {}",
            cert_err
        );

        let key_err = MtlsError::KeyNotFound(PathBuf::from("/a/key.pem"));
        assert!(
            key_err.to_string().contains("key not found"),
            "Display: {}",
            key_err
        );

        let cert_empty = MtlsError::CertEmpty(PathBuf::from("/a/cert.pem"));
        assert!(
            cert_empty.to_string().contains("empty"),
            "Display: {}",
            cert_empty
        );

        let key_empty = MtlsError::KeyEmpty(PathBuf::from("/a/key.pem"));
        assert!(
            key_empty.to_string().contains("empty"),
            "Display: {}",
            key_empty
        );
    }

    // @req REQ-ONBOARD-018
    #[test]
    fn mtls_config_equality() {
        let a = MtlsConfig {
            cert_path: PathBuf::from("/a/cert.pem"),
            key_path: PathBuf::from("/a/key.pem"),
        };
        let b = MtlsConfig {
            cert_path: PathBuf::from("/a/cert.pem"),
            key_path: PathBuf::from("/a/key.pem"),
        };
        let c = MtlsConfig {
            cert_path: PathBuf::from("/b/cert.pem"),
            key_path: PathBuf::from("/b/key.pem"),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
