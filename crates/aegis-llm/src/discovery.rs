//! Automatic LLM provider discovery and fallback (REQ-LLM-026).
//!
//! Probes available backends in priority order and returns the first
//! responsive provider configuration. This allows `aegis chat` to work
//! out of the box without requiring `aegis init` when a local model
//! server (Ollama, vLLM, TGI) is running.

use std::time::Duration;

use aegis_domain::error::DomainError;
use serde::Deserialize;

use crate::config::ProviderConfig;

/// Discovery probe timeout per endpoint.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Quick health check: probe an OpenAI-compatible endpoint.
/// Returns true if the endpoint responds to GET /models within 2 seconds.
pub async fn probe_endpoint(base_url: &str) -> bool {
    let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format!("{base_url}/models");
    matches!(client.get(&url).send().await, Ok(r) if r.status().is_success())
}

/// A provider that was discovered by probing available backends.
#[derive(Debug)]
pub struct DiscoveredProvider {
    /// Configuration suitable for passing to `create_provider`.
    pub config: ProviderConfig,
    /// Human-readable label, e.g. "Ollama (localhost:11434)".
    pub name: String,
}

/// OpenAI-compatible `/v1/models` response (minimal subset).
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

/// Single model entry from the `/v1/models` response.
#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

/// Probe available LLM backends in priority order.
///
/// Returns the first responsive provider, or an error with setup guidance.
///
/// Probe order:
/// 1. Ollama at `localhost:11434`
/// 2. vLLM / TGI at `localhost:8080`
/// 3. Vertex AI via ADC (gcloud auth)
///
/// Each probe uses a 2-second timeout. Total worst-case latency is
/// approximately 6 seconds when all probes fail.
pub async fn discover_provider() -> Result<DiscoveredProvider, DomainError> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .connect_timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| DomainError::ProviderError {
            message: format!("Failed to create discovery HTTP client: {e}"),
        })?;

    // 1. Ollama
    if let Some(discovered) =
        probe_openai_compatible(&client, "http://localhost:11434/v1", "Ollama").await
    {
        tracing::info!(
            provider = %discovered.name,
            model = %discovered.config.model,
            "discovered local provider"
        );
        return Ok(discovered);
    }

    // 2. vLLM / TGI
    if let Some(discovered) =
        probe_openai_compatible(&client, "http://localhost:8080/v1", "vLLM/TGI").await
    {
        tracing::info!(
            provider = %discovered.name,
            model = %discovered.config.model,
            "discovered local provider"
        );
        return Ok(discovered);
    }

    // 3. Vertex AI via ADC
    if let Some(discovered) = probe_vertex_ai().await {
        tracing::info!(
            provider = %discovered.name,
            "discovered cloud provider"
        );
        return Ok(discovered);
    }

    Err(DomainError::ProviderError {
        message: "No LLM backend found. To get started:\n  \
                  Local:  ollama serve && ollama pull llama3\n  \
                  Cloud:  aegis init (configure Vertex AI / Bedrock)"
            .to_string(),
    })
}

/// Probe an OpenAI-compatible endpoint by fetching its model list.
async fn probe_openai_compatible(
    client: &reqwest::Client,
    base_url: &str,
    label: &str,
) -> Option<DiscoveredProvider> {
    let url = format!("{base_url}/models");
    let response = client.get(&url).send().await.ok()?;

    if !response.status().is_success() {
        return None;
    }

    let body = response.text().await.ok()?;
    let models: ModelsResponse = serde_json::from_str(&body).ok()?;

    let model_id = models.data.first().map(|m| m.id.clone())?;
    if model_id.is_empty() {
        return None;
    }

    // Extract host:port from URL for the label
    let host_port = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches("/v1");

    Some(DiscoveredProvider {
        config: ProviderConfig::local(base_url, &model_id),
        name: format!("{label} ({host_port})"),
    })
}

/// Probe Vertex AI by checking for a valid GCP access token and project.
async fn probe_vertex_ai() -> Option<DiscoveredProvider> {
    // resolve_gcp_access_token shells out to gcloud, which is blocking.
    // Run it on a blocking thread to avoid stalling the async runtime.
    let token_result = tokio::task::spawn_blocking(crate::auth::resolve_gcp_access_token)
        .await
        .ok()?;

    let _access_token = token_result.ok()?;

    // Get project ID from gcloud config
    let project_id = tokio::task::spawn_blocking(|| {
        std::process::Command::new("gcloud")
            .args(["config", "get-value", "project"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let val = String::from_utf8(o.stdout).ok()?.trim().to_string();
                    if val.is_empty() || val == "(unset)" {
                        None
                    } else {
                        Some(val)
                    }
                } else {
                    None
                }
            })
    })
    .await
    .ok()??;

    let region = "us-central1".to_string();
    let model = "gemini-2.5-pro-001".to_string();
    let endpoint = format!(
        "https://{region}-aiplatform.googleapis.com/v1/projects/{project_id}/locations/{region}/publishers/google/models/{model}"
    );

    Some(DiscoveredProvider {
        config: ProviderConfig {
            kind: crate::config::ProviderKind::Vertex,
            model,
            endpoint,
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: Some(project_id.clone()),
            region: Some(region),
        },
        name: format!("Vertex AI ({project_id})"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper: build a valid `/v1/models` JSON response.
    fn models_response(model_ids: &[&str]) -> String {
        let data: Vec<serde_json::Value> = model_ids
            .iter()
            .map(|id| serde_json::json!({"id": id, "object": "model"}))
            .collect();
        serde_json::json!({"object": "list", "data": data}).to_string()
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    async fn discover_returns_ollama_when_localhost_11434_responds() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(models_response(&["llama3:latest", "codellama:7b"])),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .unwrap();

        let result =
            probe_openai_compatible(&client, &format!("{}/v1", server.uri()), "Ollama").await;

        let discovered = result.expect("should discover Ollama");
        assert_eq!(discovered.config.model, "llama3:latest");
        assert!(discovered.name.contains("Ollama"));
        assert_eq!(discovered.config.kind, crate::config::ProviderKind::Local);
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    async fn discover_returns_vllm_when_localhost_8080_responds() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(models_response(&["meta-llama/Llama-3-8B-Instruct"])),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .unwrap();

        let result =
            probe_openai_compatible(&client, &format!("{}/v1", server.uri()), "vLLM/TGI").await;

        let discovered = result.expect("should discover vLLM");
        assert_eq!(discovered.config.model, "meta-llama/Llama-3-8B-Instruct");
        assert!(discovered.name.contains("vLLM/TGI"));
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    async fn discover_returns_none_when_endpoint_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .unwrap();

        let result =
            probe_openai_compatible(&client, &format!("{}/v1", server.uri()), "Ollama").await;

        assert!(result.is_none());
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    async fn discover_returns_none_when_models_list_empty() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string(models_response(&[])))
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .unwrap();

        let result =
            probe_openai_compatible(&client, &format!("{}/v1", server.uri()), "Ollama").await;

        assert!(result.is_none());
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    async fn discover_returns_none_when_response_is_not_json() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string("this is not json"))
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .unwrap();

        let result =
            probe_openai_compatible(&client, &format!("{}/v1", server.uri()), "Ollama").await;

        assert!(result.is_none());
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    #[ignore] // flaky: fails when local Ollama is running
    async fn discover_returns_helpful_error_when_nothing_available() {
        // No mock servers running, so all probes will fail.
        // We call discover_provider directly -- it will try real ports
        // 11434 and 8080 which should be unoccupied in test, plus
        // gcloud which is likely not authenticated in CI.
        let result = discover_provider().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("No LLM backend found"),
            "Error should contain guidance: {err_msg}"
        );
        assert!(
            err_msg.contains("ollama serve"),
            "Error should mention Ollama: {err_msg}"
        );
        assert!(
            err_msg.contains("aegis init"),
            "Error should mention aegis init: {err_msg}"
        );
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    async fn probe_timeout_is_respected() {
        // Connect to a non-routable address to trigger a timeout.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .connect_timeout(Duration::from_millis(200))
            .build()
            .unwrap();

        let start = std::time::Instant::now();
        let result =
            probe_openai_compatible(&client, "http://192.0.2.1:11434/v1", "Ollama").await;
        let elapsed = start.elapsed();

        assert!(result.is_none());
        // Should complete within 2 seconds (generous bound for CI).
        assert!(
            elapsed < Duration::from_secs(2),
            "Probe should timeout quickly, took {elapsed:?}"
        );
    }

    // rtmx:req REQ-LLM-026
    #[test]
    fn models_response_deserializes_correctly() {
        let json = models_response(&["llama3:latest", "codellama:7b"]);
        let parsed: ModelsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.data.len(), 2);
        assert_eq!(parsed.data[0].id, "llama3:latest");
        assert_eq!(parsed.data[1].id, "codellama:7b");
    }

    // rtmx:req REQ-LLM-026
    #[tokio::test]
    async fn discover_uses_first_model_from_list() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(models_response(&["granite-3.3-2b", "llama3:latest"])),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .unwrap();

        let result =
            probe_openai_compatible(&client, &format!("{}/v1", server.uri()), "Ollama").await;

        let discovered = result.expect("should discover provider");
        assert_eq!(
            discovered.config.model, "granite-3.3-2b",
            "should use the first model in the list"
        );
    }
}
