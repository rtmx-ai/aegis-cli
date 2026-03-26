//! Provider factory: creates the right LlmProvider from config.

use aegis_domain::error::DomainError;
use aegis_domain::ports::LlmProvider;

use crate::config::{ProviderConfig, ProviderKind};
use crate::local::LocalProvider;

/// Create an `LlmProvider` from a `ProviderConfig`.
pub fn create_provider(config: &ProviderConfig) -> Result<Box<dyn LlmProvider>, DomainError> {
    match config.kind {
        ProviderKind::Local => Ok(Box::new(LocalProvider::new(config)?)),
        ProviderKind::Vertex => Err(DomainError::ProviderError {
            message: "Vertex AI provider not yet implemented".to_string(),
        }),
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

    // @req REQ-LLM-001
    #[test]
    fn factory_creates_local_provider() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let provider = create_provider(&cfg);
        assert!(provider.is_ok());
    }

    // @req REQ-LLM-001
    #[test]
    fn factory_rejects_unimplemented_providers() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-3.1-pro".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
        };
        let result = create_provider(&cfg);
        assert!(result.is_err());
    }
}
