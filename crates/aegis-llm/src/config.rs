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
