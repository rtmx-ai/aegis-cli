//! Provider configuration types.

use serde::{Deserialize, Serialize};

/// Which LLM backend to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Local,
    Vertex,
    Bedrock,
    Azure,
}

/// Configuration for an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub model: String,
    pub endpoint: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_temperature() -> f32 {
    0.0
}

impl ProviderConfig {
    /// Create a config for a local OpenAI-compatible endpoint.
    pub fn local(endpoint: &str, model: &str) -> Self {
        Self {
            kind: ProviderKind::Local,
            model: model.to_string(),
            endpoint: endpoint.to_string(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
        }
    }

    /// Validate that the model string contains a version indicator for
    /// non-local providers. A version indicator is a digit appearing after
    /// a hyphen (e.g. "gemini-2.5-pro-001", "claude-3-sonnet").
    /// Local providers allow any model string.
    pub fn validate_model_version(&self) -> Result<(), String> {
        if self.kind == ProviderKind::Local {
            return Ok(());
        }

        // Check for a digit after a hyphen anywhere in the model string.
        let has_version = self
            .model
            .split('-')
            .skip(1) // skip the part before the first hyphen
            .any(|segment| segment.starts_with(|c: char| c.is_ascii_digit()));

        if has_version {
            Ok(())
        } else {
            Err(format!(
                "model '{}' for {:?} provider must contain a version indicator \
                 (digit after hyphen, e.g. 'gemini-2.5-pro-001')",
                self.model, self.kind
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-LLM-001
    #[test]
    fn provider_config_local_constructor() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        assert_eq!(cfg.kind, ProviderKind::Local);
        assert_eq!(cfg.endpoint, "http://localhost:11434/v1");
        assert_eq!(cfg.model, "llama3");
        assert_eq!(cfg.max_tokens, 4096);
    }

    // @req REQ-LLM-001
    #[test]
    fn provider_config_deserializes_from_yaml_style_json() {
        let json = r#"{
            "kind": "local",
            "model": "granite-3.3-2b",
            "endpoint": "http://localhost:11434/v1"
        }"#;
        let cfg: ProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.kind, ProviderKind::Local);
        assert_eq!(cfg.max_tokens, 4096);
        assert_eq!(cfg.temperature, 0.0);
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_valid_vertex_with_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        };
        assert!(cfg.validate_model_version().is_ok());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_valid_bedrock_with_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Bedrock,
            model: "claude-3-sonnet-20241022".to_string(),
            endpoint: "https://bedrock.us-east-1.amazonaws.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        };
        assert!(cfg.validate_model_version().is_ok());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_valid_azure_with_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Azure,
            model: "gpt-4o-2024-05-13".to_string(),
            endpoint: "https://myendpoint.openai.azure.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        };
        assert!(cfg.validate_model_version().is_ok());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_invalid_vertex_no_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-pro".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        };
        assert!(cfg.validate_model_version().is_err());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_invalid_bedrock_no_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Bedrock,
            model: "claude-sonnet".to_string(),
            endpoint: "https://bedrock.us-east-1.amazonaws.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        };
        assert!(cfg.validate_model_version().is_err());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_local_allows_any_string() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        assert!(cfg.validate_model_version().is_ok());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_local_allows_unversioned() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "my-custom-model");
        assert!(cfg.validate_model_version().is_ok());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_invalid_no_hyphen() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "geminipro".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        };
        assert!(cfg.validate_model_version().is_err());
    }

    // @req REQ-LLM-006
    #[test]
    fn model_version_error_message_includes_model_name() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-pro".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        };
        let err = cfg.validate_model_version().unwrap_err();
        assert!(
            err.contains("gemini-pro"),
            "Error should name the model: {err}"
        );
        assert!(
            err.contains("Vertex"),
            "Error should name the provider: {err}"
        );
    }

    // @req REQ-LLM-016
    #[test]
    fn provider_kind_variants_serialize_lowercase() {
        assert_eq!(
            serde_json::to_string(&ProviderKind::Local).unwrap(),
            "\"local\""
        );
        assert_eq!(
            serde_json::to_string(&ProviderKind::Vertex).unwrap(),
            "\"vertex\""
        );
        assert_eq!(
            serde_json::to_string(&ProviderKind::Bedrock).unwrap(),
            "\"bedrock\""
        );
        assert_eq!(
            serde_json::to_string(&ProviderKind::Azure).unwrap(),
            "\"azure\""
        );
    }
}
