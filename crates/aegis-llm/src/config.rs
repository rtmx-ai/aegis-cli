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
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_read_timeout_secs")]
    pub read_timeout_secs: u64,
    /// GCP project ID (required for Vertex AI).
    #[serde(default)]
    pub project_id: Option<String>,
    /// Cloud region (e.g. "us-central1", "us-east-1").
    #[serde(default)]
    pub region: Option<String>,
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_temperature() -> f32 {
    0.0
}

fn default_connect_timeout_secs() -> u64 {
    10
}

fn default_read_timeout_secs() -> u64 {
    300
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
            connect_timeout_secs: default_connect_timeout_secs(),
            read_timeout_secs: default_read_timeout_secs(),
            project_id: None,
            region: None,
        }
    }

    /// Create a config for Vertex AI.
    pub fn vertex(project_id: &str, region: &str, model: &str) -> Self {
        let endpoint = format!(
            "https://{region}-aiplatform.googleapis.com/v1/projects/\
             {project_id}/locations/{region}/publishers/google/models/{model}"
        );
        Self {
            kind: ProviderKind::Vertex,
            model: model.to_string(),
            endpoint,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            connect_timeout_secs: default_connect_timeout_secs(),
            read_timeout_secs: default_read_timeout_secs(),
            project_id: Some(project_id.to_string()),
            region: Some(region.to_string()),
        }
    }

    /// Create a config for AWS Bedrock.
    pub fn bedrock(region: &str, model: &str) -> Self {
        let endpoint = format!("https://bedrock-runtime.{region}.amazonaws.com");
        Self {
            kind: ProviderKind::Bedrock,
            model: model.to_string(),
            endpoint,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            connect_timeout_secs: default_connect_timeout_secs(),
            read_timeout_secs: default_read_timeout_secs(),
            project_id: None,
            region: Some(region.to_string()),
        }
    }

    /// Create a config for Azure OpenAI.
    pub fn azure(endpoint: &str, model: &str) -> Self {
        Self {
            kind: ProviderKind::Azure,
            model: model.to_string(),
            endpoint: endpoint.to_string(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            connect_timeout_secs: default_connect_timeout_secs(),
            read_timeout_secs: default_read_timeout_secs(),
            project_id: None,
            region: None,
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

    // rtmx:req REQ-LLM-001
    #[test]
    fn provider_config_local_constructor() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        assert_eq!(cfg.kind, ProviderKind::Local);
        assert_eq!(cfg.endpoint, "http://localhost:11434/v1");
        assert_eq!(cfg.model, "llama3");
        assert_eq!(cfg.max_tokens, 4096);
    }

    // rtmx:req REQ-LLM-001
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

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_valid_vertex_with_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: None,
        };
        assert!(cfg.validate_model_version().is_ok());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_valid_bedrock_with_version() {
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
        assert!(cfg.validate_model_version().is_ok());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_valid_azure_with_version() {
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
        assert!(cfg.validate_model_version().is_ok());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_invalid_vertex_no_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-pro".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: None,
        };
        assert!(cfg.validate_model_version().is_err());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_invalid_bedrock_no_version() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Bedrock,
            model: "claude-sonnet".to_string(),
            endpoint: "https://bedrock.us-east-1.amazonaws.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: None,
        };
        assert!(cfg.validate_model_version().is_err());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_local_allows_any_string() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        assert!(cfg.validate_model_version().is_ok());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_local_allows_unversioned() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "my-custom-model");
        assert!(cfg.validate_model_version().is_ok());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_invalid_no_hyphen() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "geminipro".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: None,
        };
        assert!(cfg.validate_model_version().is_err());
    }

    // rtmx:req REQ-LLM-006
    #[test]
    fn model_version_error_message_includes_model_name() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-pro".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: None,
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

    // rtmx:req REQ-LLM-010
    #[test]
    fn default_timeouts_are_sensible() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        assert_eq!(cfg.connect_timeout_secs, 10);
        assert_eq!(cfg.read_timeout_secs, 300);
    }

    // rtmx:req REQ-LLM-010
    #[test]
    fn timeouts_deserialize_from_json() {
        let json = r#"{
            "kind": "local",
            "model": "llama3",
            "endpoint": "http://localhost:11434/v1",
            "connect_timeout_secs": 5,
            "read_timeout_secs": 120
        }"#;
        let cfg: ProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.connect_timeout_secs, 5);
        assert_eq!(cfg.read_timeout_secs, 120);
    }

    // rtmx:req REQ-LLM-010
    #[test]
    fn timeouts_use_defaults_when_omitted() {
        let json = r#"{
            "kind": "vertex",
            "model": "gemini-2.5-pro-001",
            "endpoint": "https://vertex.googleapis.com"
        }"#;
        let cfg: ProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.connect_timeout_secs, 10);
        assert_eq!(cfg.read_timeout_secs, 300);
    }

    // rtmx:req REQ-LLM-016
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

    // rtmx:req REQ-LLM-025
    #[test]
    fn config_with_project_id_and_region() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: Some("my-project-123".to_string()),
            region: Some("us-central1".to_string()),
        };
        assert_eq!(cfg.project_id.as_deref(), Some("my-project-123"));
        assert_eq!(cfg.region.as_deref(), Some("us-central1"));
    }

    // rtmx:req REQ-LLM-025
    #[test]
    fn config_without_project_id_and_region() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        assert!(cfg.project_id.is_none());
        assert!(cfg.region.is_none());
    }

    // rtmx:req REQ-LLM-025
    #[test]
    fn config_project_id_region_serde_roundtrip() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: Some("my-project".to_string()),
            region: Some("us-east4".to_string()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.project_id.as_deref(), Some("my-project"));
        assert_eq!(deserialized.region.as_deref(), Some("us-east4"));
    }

    // rtmx:req REQ-LLM-025
    #[test]
    fn config_project_id_region_default_to_none_in_json() {
        let json = r#"{
            "kind": "vertex",
            "model": "gemini-2.5-pro-001",
            "endpoint": "https://vertex.googleapis.com"
        }"#;
        let cfg: ProviderConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.project_id.is_none());
        assert!(cfg.region.is_none());
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn vertex_constructor_builds_full_endpoint() {
        let cfg = ProviderConfig::vertex("my-proj", "us-central1", "gemini-2.5-pro-001");
        assert_eq!(cfg.kind, ProviderKind::Vertex);
        assert_eq!(cfg.model, "gemini-2.5-pro-001");
        assert!(cfg.endpoint.contains("my-proj"));
        assert!(cfg.endpoint.contains("us-central1"));
        assert_eq!(cfg.project_id.as_deref(), Some("my-proj"));
        assert_eq!(cfg.region.as_deref(), Some("us-central1"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn bedrock_constructor_builds_regional_endpoint() {
        let cfg = ProviderConfig::bedrock("us-gov-west-1", "claude-3-sonnet-20241022");
        assert_eq!(cfg.kind, ProviderKind::Bedrock);
        assert_eq!(cfg.model, "claude-3-sonnet-20241022");
        assert!(cfg.endpoint.contains("us-gov-west-1"));
        assert_eq!(cfg.region.as_deref(), Some("us-gov-west-1"));
        assert!(cfg.project_id.is_none());
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn azure_constructor_preserves_endpoint() {
        let cfg = ProviderConfig::azure("https://myendpoint.openai.azure.com", "gpt-4o");
        assert_eq!(cfg.kind, ProviderKind::Azure);
        assert_eq!(cfg.model, "gpt-4o");
        assert_eq!(cfg.endpoint, "https://myendpoint.openai.azure.com");
        assert!(cfg.region.is_none());
    }
}
