//! Provider factory: creates the right LlmProvider from config.

use aegis_domain::error::DomainError;
use aegis_domain::ports::LlmProvider;

use crate::azure::AzureProvider;
use crate::bedrock::BedrockProvider;
use crate::config::{ProviderConfig, ProviderKind};
use crate::local::LocalProvider;
use crate::vertex::VertexProvider;

/// Create an `LlmProvider` from a `ProviderConfig`.
///
/// Resolves authentication automatically. For testable construction
/// with pre-resolved auth, use `create_provider_with_token`.
pub fn create_provider(config: &ProviderConfig) -> Result<Box<dyn LlmProvider>, DomainError> {
    match config.kind {
        ProviderKind::Local => Ok(Box::new(LocalProvider::new(config)?)),
        ProviderKind::Vertex => {
            let access_token = crate::auth::resolve_gcp_access_token()?;
            Ok(Box::new(VertexProvider::new(config, access_token)?))
        }
        ProviderKind::Bedrock => {
            let auth = crate::auth::resolve_auth(config)?;
            Ok(Box::new(BedrockProvider::new(config, auth)?))
        }
        ProviderKind::Azure => {
            let auth = crate::auth::resolve_auth(config)?;
            Ok(Box::new(AzureProvider::new(config, auth)?))
        }
    }
}

/// Create a Vertex AI provider with a pre-resolved access token.
/// Useful in tests where GCP credentials are not available.
pub fn create_vertex_provider_with_token(
    config: &ProviderConfig,
    access_token: String,
) -> Result<Box<dyn LlmProvider>, DomainError> {
    Ok(Box::new(VertexProvider::new(config, access_token)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-001
    #[test]
    fn factory_creates_local_provider() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let provider = create_provider(&cfg);
        assert!(provider.is_ok());
    }

    // rtmx:req REQ-LLM-023
    #[test]
    fn factory_creates_vertex_provider_with_token() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: Some("my-project".to_string()),
            region: Some("us-central1".to_string()),
        };
        let result = create_vertex_provider_with_token(&cfg, "ya29.fake-test-token".into());
        assert!(result.is_ok());
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn factory_creates_bedrock_provider_when_env_set() {
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE");
            std::env::set_var(
                "AWS_SECRET_ACCESS_KEY",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            );
            std::env::set_var("AWS_REGION", "us-east-1");
        }
        let cfg = ProviderConfig {
            kind: ProviderKind::Bedrock,
            model: "us.anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
            endpoint: "https://bedrock.us-east-1.amazonaws.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: None,
        };
        let result = create_provider(&cfg);
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
            std::env::remove_var("AWS_REGION");
        }
        assert!(
            result.is_ok(),
            "Expected Bedrock provider, got {:?}",
            result.err()
        );
    }
}
