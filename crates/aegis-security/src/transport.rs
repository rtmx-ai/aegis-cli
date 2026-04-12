//! Secure transport configuration for LLM API connections.
//!
//! Enforces TLS 1.3 minimum with certificate validation.
//! Supports custom CA bundles for corporate TLS inspection
//! environments (REQ-ONBOARD-007).

use aegis_domain::error::DomainError;
use reqwest::ClientBuilder;
use std::time::Duration;

/// Transport security configuration.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Minimum TLS version (always 1.3 for cloud providers).
    pub min_tls_version: TlsVersion,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Read timeout for streaming responses.
    pub read_timeout: Duration,
    /// Optional custom CA bundle path for corporate TLS inspection.
    pub ca_bundle_path: Option<String>,
}

/// Supported TLS versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    Tls12,
    Tls13,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            min_tls_version: TlsVersion::Tls13,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            ca_bundle_path: None,
        }
    }
}

impl TransportConfig {
    /// Configuration for cloud providers (strict TLS 1.3).
    pub fn cloud() -> Self {
        Self::default()
    }

    /// Configuration for local providers (relaxed for loopback).
    pub fn local() -> Self {
        Self {
            min_tls_version: TlsVersion::Tls12,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(60),
            ca_bundle_path: None,
        }
    }

    /// Build a reqwest ClientBuilder with these security settings.
    pub fn build_client(&self) -> Result<reqwest::Client, DomainError> {
        let mut builder = ClientBuilder::new()
            .connect_timeout(self.connect_timeout)
            .read_timeout(self.read_timeout)
            .https_only(self.min_tls_version == TlsVersion::Tls13);

        // Set minimum TLS version
        builder = match self.min_tls_version {
            TlsVersion::Tls13 => builder.min_tls_version(reqwest::tls::Version::TLS_1_3),
            TlsVersion::Tls12 => builder.min_tls_version(reqwest::tls::Version::TLS_1_2),
        };

        // Add custom CA bundle if configured
        if let Some(ca_path) = &self.ca_bundle_path {
            let ca_bytes = std::fs::read(ca_path).map_err(|e| DomainError::ConfigError {
                message: format!("Failed to read CA bundle {ca_path}: {e}"),
            })?;
            let cert = reqwest::Certificate::from_pem(&ca_bytes).map_err(|e| {
                DomainError::ConfigError {
                    message: format!("Invalid CA certificate: {e}"),
                }
            })?;
            builder = builder.add_root_certificate(cert);
        }

        builder.build().map_err(|e| DomainError::ProviderError {
            message: format!("Failed to build HTTP client: {e}"),
        })
    }
}

/// Validate that an endpoint URL meets security requirements.
pub fn validate_endpoint(url: &str, is_local: bool) -> Result<(), DomainError> {
    if url.is_empty() {
        return Err(DomainError::ConfigError {
            message: "Endpoint URL is empty".to_string(),
        });
    }

    if is_local {
        // Local mode: HTTP allowed only for loopback
        if url.starts_with("http://") {
            let is_loopback =
                url.contains("localhost") || url.contains("127.0.0.1") || url.contains("[::1]");
            if !is_loopback {
                return Err(DomainError::ConfigError {
                    message: format!(
                        "HTTP only allowed for loopback \
                         addresses in local mode: {url}"
                    ),
                });
            }
        }
    } else {
        // Cloud mode: HTTPS required
        if !url.starts_with("https://") {
            return Err(DomainError::ConfigError {
                message: format!("HTTPS required for cloud endpoints: {url}"),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-SECURITY-002
    #[test]
    fn cloud_config_requires_tls_13() {
        let config = TransportConfig::cloud();
        assert_eq!(config.min_tls_version, TlsVersion::Tls13);
    }

    // rtmx:req REQ-SECURITY-002
    #[test]
    fn local_config_allows_tls_12() {
        let config = TransportConfig::local();
        assert_eq!(config.min_tls_version, TlsVersion::Tls12);
    }

    // rtmx:req REQ-SECURITY-002
    #[test]
    fn cloud_config_builds_client() {
        let config = TransportConfig::cloud();
        let client = config.build_client();
        assert!(client.is_ok());
    }

    // rtmx:req REQ-SECURITY-002
    #[test]
    fn local_config_builds_client() {
        let config = TransportConfig::local();
        let client = config.build_client();
        assert!(client.is_ok());
    }

    // rtmx:req REQ-SECURITY-002
    #[test]
    fn default_timeouts_are_reasonable() {
        let config = TransportConfig::cloud();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.read_timeout, Duration::from_secs(30));
    }

    // rtmx:req REQ-ONBOARD-007
    #[test]
    fn missing_ca_bundle_returns_error() {
        let config = TransportConfig {
            ca_bundle_path: Some("/nonexistent/ca.pem".to_string()),
            ..TransportConfig::cloud()
        };
        let result = config.build_client();
        assert!(result.is_err());
    }

    // rtmx:req REQ-LLM-016
    #[test]
    fn validate_cloud_endpoint_requires_https() {
        assert!(validate_endpoint("https://vertex.googleapis.com", false).is_ok());
        assert!(validate_endpoint("http://vertex.googleapis.com", false).is_err());
    }

    // rtmx:req REQ-LLM-016
    #[test]
    fn validate_local_endpoint_allows_loopback_http() {
        assert!(validate_endpoint("http://localhost:11434", true).is_ok());
        assert!(validate_endpoint("http://127.0.0.1:11434", true).is_ok());
        assert!(validate_endpoint("http://[::1]:11434", true).is_ok());
    }

    // rtmx:req REQ-LLM-016
    #[test]
    fn validate_local_endpoint_rejects_non_loopback_http() {
        assert!(validate_endpoint("http://remote-host:8080", true).is_err());
    }

    // rtmx:req REQ-SECURITY-002
    #[test]
    fn validate_empty_endpoint_errors() {
        assert!(validate_endpoint("", false).is_err());
        assert!(validate_endpoint("", true).is_err());
    }
}
