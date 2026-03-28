//! Provider-specific authentication credential types.
//!
//! Models auth credentials for each cloud provider without performing
//! actual token exchange or refresh. Provides credential resolution
//! from `ProviderConfig` and validation that required fields are present.

use std::path::PathBuf;

use aegis_domain::error::DomainError;

use crate::config::{ProviderConfig, ProviderKind};

/// Authentication credentials for an LLM provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAuth {
    /// No authentication required (local endpoints).
    NoAuth,

    /// Simple API key authentication (local endpoints with optional key).
    ApiKey(String),

    /// Google Cloud Application Default Credentials (ADC).
    /// Uses service account JSON file or GCE metadata server.
    Gcp {
        /// Path to service account JSON key file.
        /// When `None`, ADC falls back to the metadata server
        /// or `GOOGLE_APPLICATION_CREDENTIALS` env var.
        credentials_path: Option<PathBuf>,
    },

    /// AWS Security Token Service credentials for Bedrock.
    Aws {
        access_key_id: String,
        secret_access_key: String,
        /// Temporary session token from STS AssumeRole.
        session_token: Option<String>,
        region: String,
    },

    /// Azure Entra ID (Azure AD) credentials for Azure OpenAI.
    Azure {
        tenant_id: String,
        client_id: String,
        /// Optional API key for key-based auth instead of Entra ID.
        api_key: Option<String>,
    },
}

/// Resolve the appropriate `ProviderAuth` variant from a `ProviderConfig`.
///
/// This inspects `config.kind` and returns a skeleton credential struct
/// with fields populated from environment variables where available.
/// Actual token exchange is out of scope.
pub fn resolve_auth(config: &ProviderConfig) -> Result<ProviderAuth, DomainError> {
    match config.kind {
        ProviderKind::Local => Ok(ProviderAuth::NoAuth),
        ProviderKind::Vertex => {
            let credentials_path = std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
                .ok()
                .map(PathBuf::from);
            Ok(ProviderAuth::Gcp { credentials_path })
        }
        ProviderKind::Bedrock => {
            let access_key_id =
                std::env::var("AWS_ACCESS_KEY_ID").map_err(|_| DomainError::ConfigError {
                    message: "AWS_ACCESS_KEY_ID environment variable not set".to_string(),
                })?;
            let secret_access_key =
                std::env::var("AWS_SECRET_ACCESS_KEY").map_err(|_| DomainError::ConfigError {
                    message: "AWS_SECRET_ACCESS_KEY environment variable not set".to_string(),
                })?;
            let session_token = std::env::var("AWS_SESSION_TOKEN").ok();
            let region = std::env::var("AWS_REGION").unwrap_or_else(|_| {
                std::env::var("AWS_DEFAULT_REGION").unwrap_or_else(|_| "us-east-1".to_string())
            });
            Ok(ProviderAuth::Aws {
                access_key_id,
                secret_access_key,
                session_token,
                region,
            })
        }
        ProviderKind::Azure => {
            let tenant_id =
                std::env::var("AZURE_TENANT_ID").map_err(|_| DomainError::ConfigError {
                    message: "AZURE_TENANT_ID environment variable not set".to_string(),
                })?;
            let client_id =
                std::env::var("AZURE_CLIENT_ID").map_err(|_| DomainError::ConfigError {
                    message: "AZURE_CLIENT_ID environment variable not set".to_string(),
                })?;
            let api_key = std::env::var("AZURE_OPENAI_API_KEY").ok();
            Ok(ProviderAuth::Azure {
                tenant_id,
                client_id,
                api_key,
            })
        }
    }
}

/// Validate that the credentials in a `ProviderAuth` have non-empty
/// required fields. Returns `Ok(())` if valid.
pub fn validate_auth(auth: &ProviderAuth) -> Result<(), DomainError> {
    match auth {
        ProviderAuth::NoAuth => Ok(()),
        ProviderAuth::ApiKey(key) => {
            if key.trim().is_empty() {
                return Err(DomainError::ConfigError {
                    message: "API key must not be empty".to_string(),
                });
            }
            Ok(())
        }
        ProviderAuth::Gcp { credentials_path } => {
            if let Some(path) = credentials_path
                && path.as_os_str().is_empty()
            {
                return Err(DomainError::ConfigError {
                    message: "GCP credentials path must not be empty".to_string(),
                });
            }
            Ok(())
        }
        ProviderAuth::Aws {
            access_key_id,
            secret_access_key,
            region,
            ..
        } => {
            if access_key_id.trim().is_empty() {
                return Err(DomainError::ConfigError {
                    message: "AWS access key ID must not be empty".to_string(),
                });
            }
            if secret_access_key.trim().is_empty() {
                return Err(DomainError::ConfigError {
                    message: "AWS secret access key must not be empty".to_string(),
                });
            }
            if region.trim().is_empty() {
                return Err(DomainError::ConfigError {
                    message: "AWS region must not be empty".to_string(),
                });
            }
            Ok(())
        }
        ProviderAuth::Azure {
            tenant_id,
            client_id,
            ..
        } => {
            if tenant_id.trim().is_empty() {
                return Err(DomainError::ConfigError {
                    message: "Azure tenant ID must not be empty".to_string(),
                });
            }
            if client_id.trim().is_empty() {
                return Err(DomainError::ConfigError {
                    message: "Azure client ID must not be empty".to_string(),
                });
            }
            Ok(())
        }
    }
}

/// Return the HTTP header name/value pair for the given auth credentials.
///
/// Returns `None` for auth types that do not use a simple header
/// (e.g., AWS SigV4 requires multi-header signing).
pub fn auth_header(auth: &ProviderAuth) -> Option<(String, String)> {
    match auth {
        ProviderAuth::NoAuth => None,
        ProviderAuth::ApiKey(key) => Some(("Authorization".to_string(), format!("Bearer {key}"))),
        ProviderAuth::Gcp { .. } => {
            // ADC token exchange produces a Bearer token, but the actual
            // token value requires an OAuth2 flow (out of scope).
            None
        }
        ProviderAuth::Aws { .. } => {
            // AWS SigV4 signing requires multiple headers (Authorization,
            // x-amz-date, x-amz-security-token). Not representable as a
            // single header pair.
            None
        }
        ProviderAuth::Azure { api_key, .. } => api_key
            .as_ref()
            .map(|key| ("api-key".to_string(), key.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- resolve_auth tests ---

    // @req REQ-LLM-015
    #[test]
    fn resolve_auth_local_returns_no_auth() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let auth = resolve_auth(&cfg).unwrap();
        assert_eq!(auth, ProviderAuth::NoAuth);
    }

    // @req REQ-LLM-015
    #[test]
    fn resolve_auth_vertex_returns_gcp() {
        // SAFETY: test-only; env mutation is acceptable in serial test runs.
        unsafe {
            std::env::remove_var("GOOGLE_APPLICATION_CREDENTIALS");
        }
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
        };
        let auth = resolve_auth(&cfg).unwrap();
        assert_eq!(
            auth,
            ProviderAuth::Gcp {
                credentials_path: None
            }
        );
    }

    // @req REQ-LLM-015
    #[test]
    fn resolve_auth_bedrock_fails_without_env() {
        // SAFETY: test-only; env mutation is acceptable in serial test runs.
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        }
        let cfg = ProviderConfig {
            kind: ProviderKind::Bedrock,
            model: "claude-3-sonnet-20241022".to_string(),
            endpoint: "https://bedrock.us-east-1.amazonaws.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
        };
        let result = resolve_auth(&cfg);
        assert!(result.is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn resolve_auth_azure_fails_without_env() {
        // SAFETY: test-only; env mutation is acceptable in serial test runs.
        unsafe {
            std::env::remove_var("AZURE_TENANT_ID");
            std::env::remove_var("AZURE_CLIENT_ID");
        }
        let cfg = ProviderConfig {
            kind: ProviderKind::Azure,
            model: "gpt-4o-2024-05-13".to_string(),
            endpoint: "https://myendpoint.openai.azure.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
        };
        let result = resolve_auth(&cfg);
        assert!(result.is_err());
    }

    // --- validate_auth tests ---

    // @req REQ-LLM-015
    #[test]
    fn validate_no_auth_is_ok() {
        assert!(validate_auth(&ProviderAuth::NoAuth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_api_key_ok_when_non_empty() {
        let auth = ProviderAuth::ApiKey("sk-test-123".to_string());
        assert!(validate_auth(&auth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_api_key_fails_when_empty() {
        let auth = ProviderAuth::ApiKey("".to_string());
        assert!(validate_auth(&auth).is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_api_key_fails_when_whitespace_only() {
        let auth = ProviderAuth::ApiKey("   ".to_string());
        assert!(validate_auth(&auth).is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_gcp_ok_without_path() {
        let auth = ProviderAuth::Gcp {
            credentials_path: None,
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_gcp_ok_with_valid_path() {
        let auth = ProviderAuth::Gcp {
            credentials_path: Some(PathBuf::from("/etc/gcp/sa-key.json")),
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_gcp_fails_with_empty_path() {
        let auth = ProviderAuth::Gcp {
            credentials_path: Some(PathBuf::from("")),
        };
        assert!(validate_auth(&auth).is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_aws_ok_with_all_fields() {
        let auth = ProviderAuth::Aws {
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: Some("FwoGZX...".to_string()),
            region: "us-gov-west-1".to_string(),
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_aws_ok_without_session_token() {
        let auth = ProviderAuth::Aws {
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: None,
            region: "us-east-1".to_string(),
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_aws_fails_with_empty_access_key() {
        let auth = ProviderAuth::Aws {
            access_key_id: "".to_string(),
            secret_access_key: "secret".to_string(),
            session_token: None,
            region: "us-east-1".to_string(),
        };
        assert!(validate_auth(&auth).is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_aws_fails_with_empty_secret_key() {
        let auth = ProviderAuth::Aws {
            access_key_id: "AKIA".to_string(),
            secret_access_key: "".to_string(),
            session_token: None,
            region: "us-east-1".to_string(),
        };
        assert!(validate_auth(&auth).is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_aws_fails_with_empty_region() {
        let auth = ProviderAuth::Aws {
            access_key_id: "AKIA".to_string(),
            secret_access_key: "secret".to_string(),
            session_token: None,
            region: "".to_string(),
        };
        assert!(validate_auth(&auth).is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_azure_ok_with_all_fields() {
        let auth = ProviderAuth::Azure {
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            client_id: "11111111-1111-1111-1111-111111111111".to_string(),
            api_key: Some("abc123".to_string()),
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_azure_ok_without_api_key() {
        let auth = ProviderAuth::Azure {
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            client_id: "11111111-1111-1111-1111-111111111111".to_string(),
            api_key: None,
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_azure_fails_with_empty_tenant() {
        let auth = ProviderAuth::Azure {
            tenant_id: "".to_string(),
            client_id: "client".to_string(),
            api_key: None,
        };
        assert!(validate_auth(&auth).is_err());
    }

    // @req REQ-LLM-015
    #[test]
    fn validate_azure_fails_with_empty_client() {
        let auth = ProviderAuth::Azure {
            tenant_id: "tenant".to_string(),
            client_id: "".to_string(),
            api_key: None,
        };
        assert!(validate_auth(&auth).is_err());
    }

    // --- auth_header tests ---

    // @req REQ-LLM-015
    #[test]
    fn auth_header_no_auth_returns_none() {
        assert!(auth_header(&ProviderAuth::NoAuth).is_none());
    }

    // @req REQ-LLM-015
    #[test]
    fn auth_header_api_key_returns_bearer() {
        let auth = ProviderAuth::ApiKey("sk-test".to_string());
        let header = auth_header(&auth).unwrap();
        assert_eq!(header.0, "Authorization");
        assert_eq!(header.1, "Bearer sk-test");
    }

    // @req REQ-LLM-015
    #[test]
    fn auth_header_gcp_returns_none() {
        let auth = ProviderAuth::Gcp {
            credentials_path: None,
        };
        assert!(auth_header(&auth).is_none());
    }

    // @req REQ-LLM-015
    #[test]
    fn auth_header_aws_returns_none() {
        let auth = ProviderAuth::Aws {
            access_key_id: "AKIA".to_string(),
            secret_access_key: "secret".to_string(),
            session_token: None,
            region: "us-east-1".to_string(),
        };
        assert!(auth_header(&auth).is_none());
    }

    // @req REQ-LLM-015
    #[test]
    fn auth_header_azure_with_api_key() {
        let auth = ProviderAuth::Azure {
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            api_key: Some("my-key".to_string()),
        };
        let header = auth_header(&auth).unwrap();
        assert_eq!(header.0, "api-key");
        assert_eq!(header.1, "my-key");
    }

    // @req REQ-LLM-015
    #[test]
    fn auth_header_azure_without_api_key_returns_none() {
        let auth = ProviderAuth::Azure {
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            api_key: None,
        };
        assert!(auth_header(&auth).is_none());
    }
}
