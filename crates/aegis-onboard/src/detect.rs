//! Auto-detection of local LLM providers during `aegis init`.
//!
//! Scans well-known localhost endpoints for running LLM servers:
//! - Ollama: `http://localhost:11434/api/tags`
//! - vLLM: `http://localhost:8000/v1/models`
//! - llama.cpp: `http://localhost:8080/v1/models`
//!
//! Detection is async (uses reqwest) and gracefully handles unreachable
//! endpoints. A trait-based detector allows tests to mock network calls.

use std::time::Duration;

/// A detected local LLM provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedProvider {
    /// Human-readable name (e.g., "Ollama", "vLLM", "llama.cpp").
    pub name: String,
    /// Base endpoint URL.
    pub endpoint: String,
    /// Model names available on this provider.
    pub models: Vec<String>,
}

/// Trait for probing local LLM endpoints.
///
/// Implementations can be swapped for testing without network I/O.
#[async_trait::async_trait]
pub trait ProviderDetector: Send + Sync {
    /// Probe an endpoint and return available model names, or empty
    /// vec if the endpoint is unreachable.
    async fn probe(&self, url: &str) -> Vec<String>;
}

/// Well-known local provider endpoints to scan.
pub const KNOWN_PROVIDERS: &[(&str, &str, &str)] = &[
    (
        "Ollama",
        "http://localhost:11434",
        "http://localhost:11434/api/tags",
    ),
    (
        "vLLM",
        "http://localhost:8000",
        "http://localhost:8000/v1/models",
    ),
    (
        "llama.cpp",
        "http://localhost:8080",
        "http://localhost:8080/v1/models",
    ),
];

/// HTTP-based detector that actually probes localhost endpoints.
pub struct HttpDetector {
    client: reqwest::Client,
}

impl Default for HttpDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpDetector {
    /// Create a detector with a 2-second timeout.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

#[async_trait::async_trait]
impl ProviderDetector for HttpDetector {
    async fn probe(&self, url: &str) -> Vec<String> {
        let resp = match self.client.get(url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return Vec::new(),
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        parse_model_names(url, &body)
    }
}

/// Parse model names from a JSON response.
///
/// Handles two formats:
/// - Ollama: `{"models": [{"name": "llama3:latest"}, ...]}`
/// - OpenAI-compatible: `{"data": [{"id": "llama3"}, ...]}`
fn parse_model_names(url: &str, body: &serde_json::Value) -> Vec<String> {
    // Ollama format: /api/tags returns {"models": [...]}
    if url.contains("/api/tags")
        && let Some(models) = body.get("models").and_then(|m| m.as_array())
    {
        return models
            .iter()
            .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
            .map(|s| s.to_string())
            .collect();
    }

    // OpenAI-compatible format: /v1/models returns {"data": [...]}
    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
        return data
            .iter()
            .filter_map(|m| m.get("id").and_then(|n| n.as_str()))
            .map(|s| s.to_string())
            .collect();
    }

    Vec::new()
}

/// Scan all known local endpoints and return detected providers.
pub async fn detect_local_providers(detector: &dyn ProviderDetector) -> Vec<DetectedProvider> {
    let mut results = Vec::new();

    for &(name, endpoint, probe_url) in KNOWN_PROVIDERS {
        let models = detector.probe(probe_url).await;
        if !models.is_empty() {
            results.push(DetectedProvider {
                name: name.to_string(),
                endpoint: endpoint.to_string(),
                models,
            });
        }
    }

    results
}

/// Convenience: detect using real HTTP calls.
pub async fn detect_local_providers_http() -> Vec<DetectedProvider> {
    let detector = HttpDetector::new();
    detect_local_providers(&detector).await
}

/// Detect a llama3 model among Ollama's available models.
///
/// Queries the given Ollama endpoint and looks for any model whose
/// name starts with "llama3". Returns the full model name if found.
///
/// For unit tests, accepts a list of model names directly.
pub fn detect_ollama_llama3_from_models(models: &[String]) -> Option<String> {
    // Prefer exact "llama3" or "llama3:latest", then any llama3.x variant
    let exact = models.iter().find(|m| {
        let base = m.split(':').next().unwrap_or(m);
        base == "llama3"
    });
    if let Some(m) = exact {
        return Some(m.clone());
    }

    // Look for llama3.1, llama3.2, etc.
    models
        .iter()
        .find(|m| {
            let base = m.split(':').next().unwrap_or(m);
            base.starts_with("llama3")
        })
        .cloned()
}

/// Async version that probes a live Ollama endpoint for llama3.
pub async fn detect_ollama_llama3(
    detector: &dyn ProviderDetector,
    ollama_tags_url: &str,
) -> Option<String> {
    let models = detector.probe(ollama_tags_url).await;
    detect_ollama_llama3_from_models(&models)
}

/// Mock detector for unit tests. Returns preset model lists keyed by URL.
#[cfg(test)]
pub struct MockDetector {
    responses: std::collections::HashMap<String, Vec<String>>,
}

#[cfg(test)]
impl Default for MockDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl MockDetector {
    pub fn new() -> Self {
        Self {
            responses: std::collections::HashMap::new(),
        }
    }

    pub fn with_response(mut self, url: &str, models: Vec<String>) -> Self {
        self.responses.insert(url.to_string(), models);
        self
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl ProviderDetector for MockDetector {
    async fn probe(&self, url: &str) -> Vec<String> {
        self.responses.get(url).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-ONBOARD-021
    #[tokio::test]
    async fn detect_returns_empty_when_no_providers_running() {
        let detector = MockDetector::new();
        let results = detect_local_providers(&detector).await;
        assert!(
            results.is_empty(),
            "Should return empty when no providers are reachable"
        );
    }

    // rtmx:req REQ-ONBOARD-021
    #[tokio::test]
    async fn detect_finds_ollama() {
        let detector = MockDetector::new().with_response(
            "http://localhost:11434/api/tags",
            vec!["llama3:latest".to_string(), "codellama:7b".to_string()],
        );

        let results = detect_local_providers(&detector).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Ollama");
        assert_eq!(results[0].endpoint, "http://localhost:11434");
        assert_eq!(results[0].models.len(), 2);
    }

    // rtmx:req REQ-ONBOARD-021
    #[tokio::test]
    async fn detect_finds_vllm() {
        let detector = MockDetector::new().with_response(
            "http://localhost:8000/v1/models",
            vec!["meta-llama/Llama-3-8B".to_string()],
        );

        let results = detect_local_providers(&detector).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "vLLM");
        assert_eq!(results[0].endpoint, "http://localhost:8000");
    }

    // rtmx:req REQ-ONBOARD-021
    #[tokio::test]
    async fn detect_finds_llama_cpp() {
        let detector = MockDetector::new().with_response(
            "http://localhost:8080/v1/models",
            vec!["llama3-8b-q4".to_string()],
        );

        let results = detect_local_providers(&detector).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "llama.cpp");
    }

    // rtmx:req REQ-ONBOARD-021
    #[tokio::test]
    async fn detect_finds_multiple_providers() {
        let detector = MockDetector::new()
            .with_response(
                "http://localhost:11434/api/tags",
                vec!["llama3:latest".to_string()],
            )
            .with_response(
                "http://localhost:8000/v1/models",
                vec!["codellama-13b".to_string()],
            );

        let results = detect_local_providers(&detector).await;
        assert_eq!(results.len(), 2);
    }

    // rtmx:req REQ-ONBOARD-021
    #[test]
    fn parse_ollama_model_names() {
        let body = serde_json::json!({
            "models": [
                {"name": "llama3:latest", "size": 4000000000_u64},
                {"name": "codellama:7b", "size": 3000000000_u64}
            ]
        });
        let names = parse_model_names("http://localhost:11434/api/tags", &body);
        assert_eq!(names, vec!["llama3:latest", "codellama:7b"]);
    }

    // rtmx:req REQ-ONBOARD-021
    #[test]
    fn parse_openai_compatible_model_names() {
        let body = serde_json::json!({
            "data": [
                {"id": "meta-llama/Llama-3-8B", "object": "model"},
                {"id": "codellama-13b", "object": "model"}
            ]
        });
        let names = parse_model_names("http://localhost:8000/v1/models", &body);
        assert_eq!(names, vec!["meta-llama/Llama-3-8B", "codellama-13b"]);
    }

    // rtmx:req REQ-ONBOARD-021
    #[test]
    fn parse_empty_model_list() {
        let body = serde_json::json!({"models": []});
        let names = parse_model_names("http://localhost:11434/api/tags", &body);
        assert!(names.is_empty());
    }

    // rtmx:req REQ-ONBOARD-023
    #[test]
    fn detect_llama3_exact_match() {
        let models = vec!["llama3:latest".to_string(), "codellama:7b".to_string()];
        let result = detect_ollama_llama3_from_models(&models);
        assert_eq!(result, Some("llama3:latest".to_string()));
    }

    // rtmx:req REQ-ONBOARD-023
    #[test]
    fn detect_llama3_variant() {
        let models = vec!["codellama:7b".to_string(), "llama3.2:latest".to_string()];
        let result = detect_ollama_llama3_from_models(&models);
        assert_eq!(result, Some("llama3.2:latest".to_string()));
    }

    // rtmx:req REQ-ONBOARD-023
    #[test]
    fn detect_llama3_prefers_exact() {
        let models = vec!["llama3.1:latest".to_string(), "llama3:latest".to_string()];
        let result = detect_ollama_llama3_from_models(&models);
        assert_eq!(
            result,
            Some("llama3:latest".to_string()),
            "Should prefer exact llama3 over llama3.x"
        );
    }

    // rtmx:req REQ-ONBOARD-023
    #[test]
    fn detect_llama3_not_found() {
        let models = vec!["codellama:7b".to_string(), "mistral:latest".to_string()];
        let result = detect_ollama_llama3_from_models(&models);
        assert!(result.is_none(), "Should return None when no llama3");
    }

    // rtmx:req REQ-ONBOARD-023
    #[test]
    fn detect_llama3_empty_models() {
        let result = detect_ollama_llama3_from_models(&[]);
        assert!(result.is_none());
    }

    // rtmx:req REQ-ONBOARD-023
    #[tokio::test]
    async fn detect_ollama_llama3_via_detector() {
        let detector = MockDetector::new().with_response(
            "http://localhost:11434/api/tags",
            vec!["mistral:latest".to_string(), "llama3:latest".to_string()],
        );

        let result = detect_ollama_llama3(&detector, "http://localhost:11434/api/tags").await;
        assert_eq!(result, Some("llama3:latest".to_string()));
    }

    // rtmx:req REQ-ONBOARD-023
    #[tokio::test]
    async fn detect_ollama_llama3_returns_none_when_unreachable() {
        let detector = MockDetector::new();
        let result = detect_ollama_llama3(&detector, "http://localhost:11434/api/tags").await;
        assert!(
            result.is_none(),
            "Should return None when Ollama unreachable"
        );
    }
}
