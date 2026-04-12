//! Provider factory: creates the right LlmProvider from config.

use aegis_domain::error::DomainError;
use aegis_domain::ports::LlmProvider;

use crate::config::{ProviderConfig, ProviderKind};
use crate::local::LocalProvider;
use crate::vertex::VertexProvider;

/// Create an `LlmProvider` from a `ProviderConfig`.
pub fn create_provider(config: &ProviderConfig) -> Result<Box<dyn LlmProvider>, DomainError> {
    match config.kind {
        ProviderKind::Local => Ok(Box::new(LocalProvider::new(config)?)),
        ProviderKind::Vertex => {
            let access_token = crate::auth::resolve_gcp_access_token()?;
            Ok(Box::new(VertexProvider::new(config, access_token)?))
        }
        ProviderKind::Bedrock => Err(DomainError::ProviderError {
            message: "Bedrock provider not yet implemented".to_string(),
        }),
        ProviderKind::Azure => Err(DomainError::ProviderError {
            message: "Azure OpenAI provider not yet implemented".to_string(),
        }),
    }
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
    #[ignore] // requires gcloud CLI or GCE metadata server for ADC
    fn factory_creates_vertex_provider() {
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
        let result = create_provider(&cfg);
        assert!(result.is_ok());
    }

    // rtmx:req REQ-LLM-023
    #[test]
    fn factory_rejects_unimplemented_bedrock() {
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
        let result = create_provider(&cfg);
        assert!(result.is_err());
    }
}
