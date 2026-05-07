//! Provider probe and model discovery (REQ-LLM-038, REQ-LLM-039).
//!
//! Provides two capabilities:
//! - `list_models`: enumerate available models from a provider endpoint
//! - `test_endpoint`: send a minimal completion request and measure latency
//!
//! These functions operate on ad-hoc parameters (endpoint, auth) without
//! modifying any saved configuration.

use std::time::Duration;

use serde::Deserialize;

use crate::config::ProviderKind;
use crate::rates;

/// Information about a single model available from a provider.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// Provider-assigned model identifier.
    pub model_id: String,
    /// Availability status: "available", "unauthorized", or "not_found".
    pub status: String,
    /// Input token cost per million tokens (0.0 if unknown).
    pub input_rate: f64,
    /// Output token cost per million tokens (0.0 if unknown).
    pub output_rate: f64,
}

/// Result of probing a provider endpoint with a minimal completion.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    /// HTTP status code from the completion request.
    pub status_code: u16,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u64,
    /// Model identifier that was probed.
    pub model_id: String,
    /// Endpoint URL that was probed.
    pub endpoint_url: String,
}

/// Timeout for discovery and probe HTTP requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Enumerate available models from a provider endpoint.
///
/// For local providers, calls the OpenAI-compatible GET /v1/models
/// endpoint and parses the response. For cloud providers (Vertex,
/// Bedrock, Azure), returns a placeholder stub indicating that live
/// API enumeration is not yet implemented.
///
/// Rate information is enriched from the static rate table when
/// available.
pub async fn list_models(
    provider: ProviderKind,
    endpoint: &str,
) -> Result<Vec<ModelInfo>, String> {
    match provider {
        ProviderKind::Local => list_models_openai_compatible(endpoint, "local").await,
        ProviderKind::Vertex => {
            // TODO: implement GET /v1/models for Vertex AI
            Ok(vec![ModelInfo {
                model_id: "(cloud enumeration not yet implemented)".to_string(),
                status: "not_found".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
            }])
        }
        ProviderKind::Bedrock => {
            // TODO: implement ListFoundationModels for Bedrock
            Ok(vec![ModelInfo {
                model_id: "(cloud enumeration not yet implemented)".to_string(),
                status: "not_found".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
            }])
        }
        ProviderKind::Azure => {
            // TODO: implement GET /deployments for Azure OpenAI
            Ok(vec![ModelInfo {
                model_id: "(cloud enumeration not yet implemented)".to_string(),
                status: "not_found".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
            }])
        }
    }
}

/// Query an OpenAI-compatible `/v1/models` endpoint and return model info.
async fn list_models_openai_compatible(
    endpoint: &str,
    provider_kind_str: &str,
) -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let url = format!("{endpoint}/models");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to reach {url}: {e}"))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Ok(vec![ModelInfo {
            model_id: "(endpoint requires authentication)".to_string(),
            status: "unauthorized".to_string(),
            input_rate: 0.0,
            output_rate: 0.0,
        }]);
    }
    if !status.is_success() {
        return Err(format!(
            "Endpoint returned HTTP {}: {}",
            status.as_u16(),
            url
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let models: ModelsResponse =
        serde_json::from_str(&body).map_err(|e| format!("Invalid models response: {e}"))?;

    let results = models
        .data
        .into_iter()
        .map(|entry| {
            let rate_info = rates::get_rates(provider_kind_str, &entry.id);
            ModelInfo {
                model_id: entry.id,
                status: "available".to_string(),
                input_rate: rate_info
                    .as_ref()
                    .map(|r| r.input_per_million)
                    .unwrap_or(0.0),
                output_rate: rate_info
                    .as_ref()
                    .map(|r| r.output_per_million)
                    .unwrap_or(0.0),
            }
        })
        .collect();

    Ok(results)
}

/// Send a minimal completion request to probe endpoint connectivity.
///
/// Sends a single-token completion request to the specified model and
/// measures round-trip latency. Returns the HTTP status code and timing
/// without modifying any saved configuration.
///
/// For local providers, uses the OpenAI-compatible chat completions API.
/// For cloud providers, returns a stub result indicating that live
/// probing is not yet implemented.
pub async fn test_endpoint(
    provider: ProviderKind,
    endpoint: &str,
    model: &str,
) -> Result<ProbeResult, String> {
    match provider {
        ProviderKind::Local => test_endpoint_openai_compatible(endpoint, model).await,
        ProviderKind::Vertex | ProviderKind::Bedrock | ProviderKind::Azure => {
            // TODO: implement cloud provider probing with auth
            Err(format!(
                "Cloud provider probing for {:?} is not yet implemented. \
                 Use --provider local for now.",
                provider
            ))
        }
    }
}

/// Probe an OpenAI-compatible chat completions endpoint.
async fn test_endpoint_openai_compatible(
    endpoint: &str,
    model: &str,
) -> Result<ProbeResult, String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let url = format!("{endpoint}/chat/completions");
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1
    });

    let start = std::time::Instant::now();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to reach {url}: {e}"))?;
    let latency = start.elapsed();

    let status_code = response.status().as_u16();

    Ok(ProbeResult {
        status_code,
        latency_ms: latency.as_millis() as u64,
        model_id: model.to_string(),
        endpoint_url: endpoint.to_string(),
    })
}

/// Format a list of ModelInfo as a human-readable table.
pub fn format_model_table(models: &[ModelInfo]) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{:<45} {:<15} {:>12} {:>12}",
        "MODEL_ID", "STATUS", "INPUT_RATE", "OUTPUT_RATE"
    ));
    lines.push(format!("{:-<45} {:-<15} {:->12} {:->12}", "", "", "", ""));
    for m in models {
        let input = if m.input_rate == 0.0 {
            "--".to_string()
        } else {
            format!("${:.2}/M", m.input_rate)
        };
        let output = if m.output_rate == 0.0 {
            "--".to_string()
        } else {
            format!("${:.2}/M", m.output_rate)
        };
        lines.push(format!(
            "{:<45} {:<15} {:>12} {:>12}",
            m.model_id, m.status, input, output
        ));
    }
    lines.join("\n")
}

/// Format a ProbeResult as a human-readable summary.
pub fn format_probe_result(result: &ProbeResult) -> String {
    let status_label = if result.status_code == 200 {
        "OK"
    } else {
        "FAIL"
    };
    format!(
        "endpoint: {}\n\
         model:    {}\n\
         status:   {} (HTTP {})\n\
         latency:  {} ms",
        result.endpoint_url, result.model_id, status_label, result.status_code, result.latency_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper: build a valid `/v1/models` JSON response.
    fn models_json(model_ids: &[&str]) -> String {
        let data: Vec<serde_json::Value> = model_ids
            .iter()
            .map(|id| serde_json::json!({"id": id, "object": "model"}))
            .collect();
        serde_json::json!({"object": "list", "data": data}).to_string()
    }

    // rtmx:req REQ-LLM-038
    #[tokio::test]
    async fn list_models_local_returns_available_models() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(models_json(&["llama3:latest", "codellama:7b"])),
            )
            .mount(&server)
            .await;

        let endpoint = format!("{}/v1", server.uri());
        let result = list_models(ProviderKind::Local, &endpoint).await;

        let models = result.expect("list_models should succeed");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model_id, "llama3:latest");
        assert_eq!(models[0].status, "available");
        assert_eq!(models[1].model_id, "codellama:7b");
        assert_eq!(models[1].status, "available");
    }

    // rtmx:req REQ-LLM-038
    #[tokio::test]
    async fn list_models_local_enriches_rates() {
        let server = MockServer::start().await;

        // Local models have $0/$0 rates
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(models_json(&["llama3:latest"])),
            )
            .mount(&server)
            .await;

        let endpoint = format!("{}/v1", server.uri());
        let models = list_models(ProviderKind::Local, &endpoint)
            .await
            .expect("should succeed");

        assert_eq!(models.len(), 1);
        // Local models are free
        assert!((models[0].input_rate).abs() < f64::EPSILON);
        assert!((models[0].output_rate).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-038
    #[tokio::test]
    async fn list_models_returns_unauthorized_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let endpoint = format!("{}/v1", server.uri());
        let models = list_models(ProviderKind::Local, &endpoint)
            .await
            .expect("should succeed with unauthorized status");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].status, "unauthorized");
    }

    // rtmx:req REQ-LLM-038
    #[tokio::test]
    async fn list_models_returns_error_on_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let endpoint = format!("{}/v1", server.uri());
        let result = list_models(ProviderKind::Local, &endpoint).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP 500"));
    }

    // rtmx:req REQ-LLM-038
    #[tokio::test]
    async fn list_models_returns_error_on_unreachable() {
        let result = list_models(ProviderKind::Local, "http://127.0.0.1:1/v1").await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-LLM-038
    #[tokio::test]
    async fn list_models_cloud_returns_stub() {
        let result = list_models(ProviderKind::Vertex, "unused").await;
        let models = result.expect("cloud stub should succeed");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].status, "not_found");
    }

    // rtmx:req REQ-LLM-038
    #[test]
    fn format_model_table_produces_aligned_output() {
        let models = vec![
            ModelInfo {
                model_id: "llama3:latest".to_string(),
                status: "available".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
            },
            ModelInfo {
                model_id: "gemini-2.5-pro-001".to_string(),
                status: "available".to_string(),
                input_rate: 1.25,
                output_rate: 10.0,
            },
        ];
        let table = format_model_table(&models);
        assert!(table.contains("MODEL_ID"));
        assert!(table.contains("llama3:latest"));
        assert!(table.contains("gemini-2.5-pro-001"));
        assert!(table.contains("$1.25/M"));
        assert!(table.contains("$10.00/M"));
        assert!(table.contains("--")); // free rate marker
    }

    // rtmx:req REQ-LLM-039
    #[tokio::test]
    async fn test_endpoint_local_returns_probe_result() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"choices":[{"message":{"content":"ok"}}]}"#),
            )
            .mount(&server)
            .await;

        let endpoint = format!("{}/v1", server.uri());
        let result = test_endpoint(ProviderKind::Local, &endpoint, "llama3")
            .await
            .expect("probe should succeed");

        assert_eq!(result.status_code, 200);
        assert_eq!(result.model_id, "llama3");
        assert!(result.endpoint_url.contains(&server.uri()));
        // Latency should be positive (at least 0ms for loopback)
        assert!(result.latency_ms < 10_000, "latency too high");
    }

    // rtmx:req REQ-LLM-039
    #[tokio::test]
    async fn test_endpoint_local_reports_failure_status() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let endpoint = format!("{}/v1", server.uri());
        let result = test_endpoint(ProviderKind::Local, &endpoint, "nonexistent")
            .await
            .expect("probe should return result even on 404");

        assert_eq!(result.status_code, 404);
        assert_eq!(result.model_id, "nonexistent");
    }

    // rtmx:req REQ-LLM-039
    #[tokio::test]
    async fn test_endpoint_unreachable_returns_error() {
        let result = test_endpoint(ProviderKind::Local, "http://127.0.0.1:1/v1", "llama3").await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-LLM-039
    #[tokio::test]
    async fn test_endpoint_cloud_returns_not_implemented() {
        let result = test_endpoint(ProviderKind::Vertex, "unused", "gemini-2.5-pro-001").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not yet implemented"));
    }

    // rtmx:req REQ-LLM-039
    #[test]
    fn format_probe_result_success() {
        let result = ProbeResult {
            status_code: 200,
            latency_ms: 42,
            model_id: "llama3".to_string(),
            endpoint_url: "http://localhost:11434/v1".to_string(),
        };
        let output = format_probe_result(&result);
        assert!(output.contains("OK"));
        assert!(output.contains("200"));
        assert!(output.contains("42 ms"));
        assert!(output.contains("llama3"));
    }

    // rtmx:req REQ-LLM-039
    #[test]
    fn format_probe_result_failure() {
        let result = ProbeResult {
            status_code: 401,
            latency_ms: 15,
            model_id: "gpt-4o".to_string(),
            endpoint_url: "https://myendpoint.openai.azure.com/v1".to_string(),
        };
        let output = format_probe_result(&result);
        assert!(output.contains("FAIL"));
        assert!(output.contains("401"));
    }

    // rtmx:req REQ-LLM-038
    #[test]
    fn model_info_struct_fields() {
        let info = ModelInfo {
            model_id: "test".to_string(),
            status: "available".to_string(),
            input_rate: 1.0,
            output_rate: 2.0,
        };
        assert_eq!(info.model_id, "test");
        assert_eq!(info.status, "available");
        assert!((info.input_rate - 1.0).abs() < f64::EPSILON);
        assert!((info.output_rate - 2.0).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-039
    #[test]
    fn probe_result_struct_fields() {
        let result = ProbeResult {
            status_code: 200,
            latency_ms: 100,
            model_id: "test-model".to_string(),
            endpoint_url: "http://localhost/v1".to_string(),
        };
        assert_eq!(result.status_code, 200);
        assert_eq!(result.latency_ms, 100);
        assert_eq!(result.model_id, "test-model");
        assert_eq!(result.endpoint_url, "http://localhost/v1");
    }
}
