//! Provider-specific authentication credential types.
//!
//! Models auth credentials for each cloud provider. For GCP/Vertex AI,
//! performs ADC access-token resolution via `gcloud` CLI or GCE metadata
//! server. Provides credential resolution from `ProviderConfig` and
//! validation that required fields are present.

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
    /// Carries a resolved OAuth2 access token obtained via `gcloud`
    /// CLI or the GCE metadata server.
    Gcp {
        /// OAuth2 bearer token for Vertex AI requests.
        access_token: String,
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

/// Resolve a GCP OAuth2 access token using Application Default Credentials.
///
/// Strategy (in order):
/// 1. Shell out to `gcloud auth print-access-token`. Works when the user has
///    run `gcloud auth login`, `gcloud auth application-default login`, or
///    has `GOOGLE_APPLICATION_CREDENTIALS` set to a service-account key.
/// 2. Fall back to the GCE metadata server at
///    `http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token`.
///    Works on GCE, GKE, and Cloud Run.
/// 3. If both fail, return a `DomainError::ProviderError` with guidance.
pub fn resolve_gcp_access_token() -> Result<String, DomainError> {
    // Strategy 1: gcloud CLI
    if let Some(token) = try_gcloud_access_token() {
        return Ok(token);
    }

    // Strategy 2: GCE metadata server
    if let Some(token) = try_metadata_server_token() {
        return Ok(token);
    }

    Err(DomainError::ProviderError {
        message: "Failed to obtain GCP access token. Ensure you have \
                  authenticated via `gcloud auth application-default login` \
                  or are running on a GCE/GKE instance with a service account."
            .to_string(),
    })
}

/// Attempt to obtain a token via `gcloud auth print-access-token`.
fn try_gcloud_access_token() -> Option<String> {
    let output = std::process::Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        return None;
    }

    Some(token)
}

/// Attempt to obtain a token from the GCE metadata server.
fn try_metadata_server_token() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args([
            "--silent",
            "--fail",
            "--max-time",
            "2",
            "--header",
            "Metadata-Flavor: Google",
            "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8(output.stdout).ok()?;
    // Response is JSON: {"access_token":"...","expires_in":3600,"token_type":"Bearer"}
    // Parse minimally without pulling in serde_json at this layer.
    extract_access_token_from_json(&body)
}

/// Extract the `access_token` value from a GCE metadata JSON response.
///
/// Avoids a serde_json dependency by doing simple string parsing.
fn extract_access_token_from_json(json: &str) -> Option<String> {
    let marker = "\"access_token\":\"";
    let start = json.find(marker)? + marker.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    let token = &rest[..end];
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

/// Resolve the appropriate `ProviderAuth` variant from a `ProviderConfig`.
///
/// This inspects `config.kind` and returns a credential struct with fields
/// populated from environment variables or token exchange where applicable.
/// For Vertex AI, performs ADC access-token resolution.
pub fn resolve_auth(config: &ProviderConfig) -> Result<ProviderAuth, DomainError> {
    match config.kind {
        ProviderKind::Local => Ok(ProviderAuth::NoAuth),
        ProviderKind::Vertex => {
            let access_token = resolve_gcp_access_token()?;
            Ok(ProviderAuth::Gcp { access_token })
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
        ProviderAuth::Gcp { access_token } => {
            if access_token.trim().is_empty() {
                return Err(DomainError::ConfigError {
                    message: "GCP access token must not be empty".to_string(),
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
        ProviderAuth::Gcp { access_token } => Some((
            "Authorization".to_string(),
            format!("Bearer {access_token}"),
        )),
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

    // rtmx:req REQ-LLM-015
    #[test]
    fn resolve_auth_local_returns_no_auth() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let auth = resolve_auth(&cfg).unwrap();
        assert_eq!(auth, ProviderAuth::NoAuth);
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn gcp_auth_variant_validates_non_empty_token() {
        // Verify that the Gcp auth variant passes validation when a
        // token is present (the real resolve_gcp_access_token flow
        // is tested via extract_access_token_from_json below).
        let auth = ProviderAuth::Gcp {
            access_token: "ya29.test-token".to_string(),
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn gcp_auth_variant_rejects_empty_token() {
        let auth = ProviderAuth::Gcp {
            access_token: "  ".to_string(),
        };
        assert!(validate_auth(&auth).is_err());
    }

    // rtmx:req REQ-LLM-015
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
            project_id: None,
            region: None,
        };
        let result = resolve_auth(&cfg);
        assert!(result.is_err());
    }

    // rtmx:req REQ-LLM-015
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
            project_id: None,
            region: None,
        };
        let result = resolve_auth(&cfg);
        assert!(result.is_err());
    }

    // --- validate_auth tests ---

    // rtmx:req REQ-LLM-015
    #[test]
    fn validate_no_auth_is_ok() {
        assert!(validate_auth(&ProviderAuth::NoAuth).is_ok());
    }

    // rtmx:req REQ-LLM-015
    #[test]
    fn validate_api_key_ok_when_non_empty() {
        let auth = ProviderAuth::ApiKey("sk-test-123".to_string());
        assert!(validate_auth(&auth).is_ok());
    }

    // rtmx:req REQ-LLM-015
    #[test]
    fn validate_api_key_fails_when_empty() {
        let auth = ProviderAuth::ApiKey("".to_string());
        assert!(validate_auth(&auth).is_err());
    }

    // rtmx:req REQ-LLM-015
    #[test]
    fn validate_api_key_fails_when_whitespace_only() {
        let auth = ProviderAuth::ApiKey("   ".to_string());
        assert!(validate_auth(&auth).is_err());
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn validate_gcp_ok_with_token() {
        let auth = ProviderAuth::Gcp {
            access_token: "ya29.a0ARrdaM_example_token".to_string(),
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn validate_gcp_fails_with_empty_token() {
        let auth = ProviderAuth::Gcp {
            access_token: "".to_string(),
        };
        assert!(validate_auth(&auth).is_err());
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn validate_gcp_fails_with_whitespace_token() {
        let auth = ProviderAuth::Gcp {
            access_token: "   ".to_string(),
        };
        assert!(validate_auth(&auth).is_err());
    }

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
    #[test]
    fn validate_azure_ok_with_all_fields() {
        let auth = ProviderAuth::Azure {
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            client_id: "11111111-1111-1111-1111-111111111111".to_string(),
            api_key: Some("abc123".to_string()),
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // rtmx:req REQ-LLM-015
    #[test]
    fn validate_azure_ok_without_api_key() {
        let auth = ProviderAuth::Azure {
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            client_id: "11111111-1111-1111-1111-111111111111".to_string(),
            api_key: None,
        };
        assert!(validate_auth(&auth).is_ok());
    }

    // rtmx:req REQ-LLM-015
    #[test]
    fn validate_azure_fails_with_empty_tenant() {
        let auth = ProviderAuth::Azure {
            tenant_id: "".to_string(),
            client_id: "client".to_string(),
            api_key: None,
        };
        assert!(validate_auth(&auth).is_err());
    }

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
    #[test]
    fn auth_header_no_auth_returns_none() {
        assert!(auth_header(&ProviderAuth::NoAuth).is_none());
    }

    // rtmx:req REQ-LLM-015
    #[test]
    fn auth_header_api_key_returns_bearer() {
        let auth = ProviderAuth::ApiKey("sk-test".to_string());
        let header = auth_header(&auth).unwrap();
        assert_eq!(header.0, "Authorization");
        assert_eq!(header.1, "Bearer sk-test");
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn auth_header_gcp_returns_bearer_token() {
        let auth = ProviderAuth::Gcp {
            access_token: "ya29.test-token".to_string(),
        };
        let header = auth_header(&auth).unwrap();
        assert_eq!(header.0, "Authorization");
        assert_eq!(header.1, "Bearer ya29.test-token");
    }

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
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

    // rtmx:req REQ-LLM-015
    #[test]
    fn auth_header_azure_without_api_key_returns_none() {
        let auth = ProviderAuth::Azure {
            tenant_id: "tenant".to_string(),
            client_id: "client".to_string(),
            api_key: None,
        };
        assert!(auth_header(&auth).is_none());
    }

    // --- resolve_gcp_access_token tests ---

    // rtmx:req REQ-LLM-021
    #[test]
    fn metadata_server_token_parsed_correctly() {
        // Simulate the GCE metadata response that resolve_gcp_access_token
        // would receive. The parsing flow (extract_access_token_from_json)
        // is the offline-testable contract.
        let metadata_response =
            r#"{"access_token":"ya29.c.b0AXv0zTPtest","expires_in":3599,"token_type":"Bearer"}"#;
        let token = extract_access_token_from_json(metadata_response).unwrap();
        assert!(!token.trim().is_empty(), "token must not be empty");
        assert!(
            token.starts_with("ya29."),
            "expected ya29. prefix, got: {token}"
        );
    }

    // --- extract_access_token_from_json tests ---

    // rtmx:req REQ-LLM-021
    #[test]
    fn extract_token_from_valid_json() {
        let json = r#"{"access_token":"ya29.test123","expires_in":3600,"token_type":"Bearer"}"#;
        let token = extract_access_token_from_json(json).unwrap();
        assert_eq!(token, "ya29.test123");
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn extract_token_from_json_with_spaces() {
        let json = r#"{ "access_token" : "ya29.spaced" , "expires_in" : 3600 }"#;
        // Our parser expects no space after the colon in the marker,
        // so this returns None (metadata server returns compact JSON).
        assert!(extract_access_token_from_json(json).is_none());
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn extract_token_from_empty_json() {
        assert!(extract_access_token_from_json("").is_none());
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn extract_token_from_json_with_empty_token() {
        let json = r#"{"access_token":"","expires_in":3600}"#;
        assert!(extract_access_token_from_json(json).is_none());
    }

    // rtmx:req REQ-LLM-021
    #[test]
    fn extract_token_missing_field() {
        let json = r#"{"expires_in":3600,"token_type":"Bearer"}"#;
        assert!(extract_access_token_from_json(json).is_none());
    }
}
