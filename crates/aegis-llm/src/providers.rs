//! Provider probe and model discovery (REQ-LLM-038, REQ-LLM-039, REQ-LLM-040).
//!
//! Provides three capabilities:
//! - `list_models`: enumerate available models from a provider endpoint
//! - `test_endpoint`: send a minimal completion request and measure latency
//! - `list_providers`: query provider registry returning model metadata
//!
//! These functions operate on ad-hoc parameters (endpoint, auth) without
//! modifying any saved configuration.

use std::time::Duration;

use serde::Deserialize;

use crate::config::ProviderKind;
use crate::rates;

// ---------------------------------------------------------------------------
// Provider registry types (REQ-LLM-040)
// ---------------------------------------------------------------------------

/// Summary of a configured provider for the `aegis providers list` command.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub endpoint: String,
    pub models: Vec<RegistryModelInfo>,
    pub status: ProviderStatus,
}

/// Model metadata returned by the provider registry query.
///
/// Named `RegistryModelInfo` to avoid collision with the existing
/// discovery-oriented [`ModelInfo`] struct in this module.
#[derive(Debug, Clone)]
pub struct RegistryModelInfo {
    pub model_id: String,
    pub context_window: Option<u32>,
}

/// Whether a provider is configured in the current config.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderStatus {
    Configured,
    NotConfigured,
}

/// Minimal config input for [`list_providers`].
///
/// This avoids a crate dependency on `aegis-onboard` (which owns
/// `AegisConfig`). Callers construct this from `AegisConfig` fields
/// at the composition root.
#[derive(Debug, Clone)]
pub struct ProviderRegistryInput {
    /// The configured provider string (e.g. "local", "vertex", "bedrock", "azure").
    pub provider: String,
    /// The configured model identifier.
    pub model: String,
    /// The configured endpoint URL.
    pub endpoint: String,
    /// Optional cloud region (e.g. "us-central1").
    pub region: Option<String>,
}

/// Query the provider registry and return metadata for all known providers.
///
/// For each known provider kind (local, vertex, bedrock, azure), checks
/// whether it matches the currently configured provider and populates
/// a [`ProviderInfo`] accordingly. The configured provider gets its
/// endpoint and model from the input; unconfigured providers get
/// placeholder values.
pub fn list_providers(input: &ProviderRegistryInput) -> Vec<ProviderInfo> {
    let configured_kind = input.provider.to_lowercase();

    let known_providers: Vec<(&str, &str)> = vec![
        ("local", "http://localhost:11434/v1"),
        ("vertex", "https://{region}-aiplatform.googleapis.com/v1"),
        ("bedrock", "https://bedrock-runtime.{region}.amazonaws.com"),
        ("azure", "https://{deployment}.openai.azure.com"),
    ];

    known_providers
        .into_iter()
        .map(|(name, default_endpoint)| {
            if configured_kind == name {
                ProviderInfo {
                    name: name.to_string(),
                    endpoint: input.endpoint.clone(),
                    models: vec![RegistryModelInfo {
                        model_id: input.model.clone(),
                        context_window: default_context_window(&input.model),
                    }],
                    status: ProviderStatus::Configured,
                }
            } else {
                let endpoint = if let Some(ref region) = input.region {
                    default_endpoint.replace("{region}", region)
                } else {
                    default_endpoint.replace("{region}", "us-central1")
                };
                ProviderInfo {
                    name: name.to_string(),
                    endpoint,
                    models: vec![],
                    status: ProviderStatus::NotConfigured,
                }
            }
        })
        .collect()
}

/// Return a default context window for well-known model families.
fn default_context_window(model_id: &str) -> Option<u32> {
    let id = model_id.to_lowercase();
    if id.contains("gemini") {
        Some(1_000_000)
    } else if id.contains("claude") {
        Some(200_000)
    } else if id.contains("gpt-4") {
        Some(128_000)
    } else if id.contains("llama") {
        Some(8_192)
    } else if id.contains("mixtral") {
        Some(32_768)
    } else {
        None
    }
}

/// Information about a single model available from a provider.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// Provider-assigned model identifier.
    pub model_id: String,
    /// Availability status: "available", "unauthorized", "restricted", or "not_found".
    pub status: String,
    /// Input token cost per million tokens (0.0 if unknown).
    pub input_rate: f64,
    /// Output token cost per million tokens (0.0 if unknown).
    pub output_rate: f64,
    /// Country of origin (populated by origin policy filter).
    pub origin: Option<String>,
    /// If restricted, the reason why.
    pub restriction_reason: Option<String>,
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
                origin: None,
                restriction_reason: None,
            }])
        }
        ProviderKind::Bedrock => {
            // TODO: implement ListFoundationModels for Bedrock
            Ok(vec![ModelInfo {
                model_id: "(cloud enumeration not yet implemented)".to_string(),
                status: "not_found".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
                origin: None,
                restriction_reason: None,
            }])
        }
        ProviderKind::Azure => {
            // TODO: implement GET /deployments for Azure OpenAI
            Ok(vec![ModelInfo {
                model_id: "(cloud enumeration not yet implemented)".to_string(),
                status: "not_found".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
                origin: None,
                restriction_reason: None,
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
            origin: None,
            restriction_reason: None,
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
                origin: None,
                restriction_reason: None,
            }
        })
        .collect();

    Ok(results)
}

/// Apply model origin policy to a list of discovered models (REQ-LLM-045).
///
/// Annotates each model with its country of origin and policy tier.
/// Models denied by policy get status changed to "restricted" with a
/// reason string. Models already in a non-available state (e.g.
/// "unauthorized", "not_found") are left unchanged.
pub fn apply_origin_policy(
    models: &mut [ModelInfo],
    policy: &crate::model_origin::ModelOriginPolicy,
) {
    for model in models.iter_mut() {
        let decision = policy.evaluate(&model.model_id);
        model.origin = Some(decision.origin.to_string());
        if !decision.is_allowed() && model.status == "available" {
            model.status = "restricted".to_string();
            model.restriction_reason = Some(decision.reason);
        }
    }
}

/// Check whether a model switch should be allowed (REQ-LLM-046).
///
/// Returns `Ok(())` if the model is permitted, or `Err(reason)` if
/// the model is denied by the origin policy.
pub fn check_model_switch(
    model: &str,
    policy: &crate::model_origin::ModelOriginPolicy,
) -> Result<(), String> {
    let decision = policy.evaluate(model);
    if decision.is_allowed() {
        Ok(())
    } else {
        Err(decision.reason)
    }
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

/// Format provider info as a CLI-friendly table.
///
/// Output format:
/// ```text
/// Provider    | Model              | Context Window | Status
/// ------------|--------------------|-----------------|-----------
/// vertex      | gemini-2.5-pro     | 1,000,000      | Configured
/// bedrock     |                    |                 | Not Configured
/// ```
pub fn format_providers_table(providers: &[ProviderInfo]) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{:<12} | {:<25} | {:>14} | {}",
        "Provider", "Model", "Context Window", "Status"
    ));
    lines.push(format!(
        "{:-<12}-+-{:-<25}-+-{:->14}-+-{:-<14}",
        "", "", "", ""
    ));
    for p in providers {
        let status_str = match &p.status {
            ProviderStatus::Configured => "Configured",
            ProviderStatus::NotConfigured => "Not Configured",
        };
        if p.models.is_empty() {
            lines.push(format!(
                "{:<12} | {:<25} | {:>14} | {}",
                p.name, "", "", status_str
            ));
        } else {
            for (i, m) in p.models.iter().enumerate() {
                let name = if i == 0 { p.name.as_str() } else { "" };
                let ctx = match m.context_window {
                    Some(w) => format_context_window(w),
                    None => String::new(),
                };
                let status = if i == 0 { status_str } else { "" };
                lines.push(format!(
                    "{:<12} | {:<25} | {:>14} | {}",
                    name, m.model_id, ctx, status
                ));
            }
        }
    }
    lines.join("\n")
}

/// Format a context window size with thousands separators.
fn format_context_window(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
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
                origin: None,
                restriction_reason: None,
            },
            ModelInfo {
                model_id: "gemini-2.5-pro-001".to_string(),
                status: "available".to_string(),
                input_rate: 1.25,
                output_rate: 10.0,
                origin: None,
                restriction_reason: None,
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
            origin: None,
            restriction_reason: None,
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

    // rtmx:req REQ-LLM-040
    #[test]
    fn test_list_providers_returns_metadata() {
        let input = ProviderRegistryInput {
            provider: "local".to_string(),
            model: "llama3:latest".to_string(),
            endpoint: "http://localhost:11434/v1".to_string(),
            region: None,
        };

        let providers = list_providers(&input);

        // Should return all four known provider kinds
        assert_eq!(providers.len(), 4);

        // The configured provider (local) should be Configured with model info
        let local = providers.iter().find(|p| p.name == "local").unwrap();
        assert_eq!(local.status, ProviderStatus::Configured);
        assert_eq!(local.endpoint, "http://localhost:11434/v1");
        assert_eq!(local.models.len(), 1);
        assert_eq!(local.models[0].model_id, "llama3:latest");
        assert_eq!(local.models[0].context_window, Some(8_192));

        // Unconfigured providers should be NotConfigured with no models
        let vertex = providers.iter().find(|p| p.name == "vertex").unwrap();
        assert_eq!(vertex.status, ProviderStatus::NotConfigured);
        assert!(vertex.models.is_empty());

        let bedrock = providers.iter().find(|p| p.name == "bedrock").unwrap();
        assert_eq!(bedrock.status, ProviderStatus::NotConfigured);
        assert!(bedrock.models.is_empty());

        let azure = providers.iter().find(|p| p.name == "azure").unwrap();
        assert_eq!(azure.status, ProviderStatus::NotConfigured);
        assert!(azure.models.is_empty());
    }

    // rtmx:req REQ-LLM-040
    #[test]
    fn test_list_providers_vertex_configured() {
        let input = ProviderRegistryInput {
            provider: "vertex".to_string(),
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://us-central1-aiplatform.googleapis.com/v1".to_string(),
            region: Some("us-central1".to_string()),
        };

        let providers = list_providers(&input);

        let vertex = providers.iter().find(|p| p.name == "vertex").unwrap();
        assert_eq!(vertex.status, ProviderStatus::Configured);
        assert_eq!(
            vertex.endpoint,
            "https://us-central1-aiplatform.googleapis.com/v1"
        );
        assert_eq!(vertex.models.len(), 1);
        assert_eq!(vertex.models[0].model_id, "gemini-2.5-pro-001");
        assert_eq!(vertex.models[0].context_window, Some(1_000_000));

        // local should be NotConfigured
        let local = providers.iter().find(|p| p.name == "local").unwrap();
        assert_eq!(local.status, ProviderStatus::NotConfigured);
    }

    // rtmx:req REQ-LLM-041
    #[test]
    fn test_providers_list_table_format() {
        let providers = list_providers(&ProviderRegistryInput {
            provider: "local".to_string(),
            model: "llama3".to_string(),
            endpoint: "http://localhost:11434".to_string(),
            region: None,
        });
        let table = format_providers_table(&providers);
        assert!(table.contains("Provider"));
        assert!(table.contains("local"));
        assert!(table.contains("Configured"));
        assert!(table.contains("Not Configured"));
    }

    // rtmx:req REQ-LLM-041
    #[test]
    fn test_format_context_window_thousands_separator() {
        assert_eq!(format_context_window(1_000_000), "1,000,000");
        assert_eq!(format_context_window(8_192), "8,192");
        assert_eq!(format_context_window(128_000), "128,000");
        assert_eq!(format_context_window(100), "100");
    }

    // rtmx:req REQ-LLM-040
    #[test]
    fn test_list_providers_unknown_model_has_no_context_window() {
        let input = ProviderRegistryInput {
            provider: "local".to_string(),
            model: "my-custom-model".to_string(),
            endpoint: "http://localhost:8080/v1".to_string(),
            region: None,
        };

        let providers = list_providers(&input);
        let local = providers.iter().find(|p| p.name == "local").unwrap();
        assert_eq!(local.models[0].model_id, "my-custom-model");
        assert_eq!(local.models[0].context_window, None);
    }

    // --- REQ-LLM-045: Discovery filter strips or tags restricted models ---

    // rtmx:req REQ-LLM-045
    #[test]
    fn test_list_models_filters_restricted_origins() {
        use crate::model_origin::ModelOriginPolicy;

        let mut models = vec![
            ModelInfo {
                model_id: "llama3:latest".to_string(),
                status: "available".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
                origin: None,
                restriction_reason: None,
            },
            ModelInfo {
                model_id: "qwen:7b".to_string(),
                status: "available".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
                origin: None,
                restriction_reason: None,
            },
            ModelInfo {
                model_id: "deepseek-r1:8b".to_string(),
                status: "available".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
                origin: None,
                restriction_reason: None,
            },
            ModelInfo {
                model_id: "mistral:7b".to_string(),
                status: "available".to_string(),
                input_rate: 0.0,
                output_rate: 0.0,
                origin: None,
                restriction_reason: None,
            },
        ];

        let policy = ModelOriginPolicy::default();
        apply_origin_policy(&mut models, &policy);

        // US model: approved
        assert_eq!(models[0].status, "available");
        assert_eq!(models[0].origin.as_deref(), Some("US"));
        assert!(models[0].restriction_reason.is_none());

        // Chinese models: restricted
        assert_eq!(models[1].status, "restricted");
        assert_eq!(models[1].origin.as_deref(), Some("China"));
        assert!(
            models[1]
                .restriction_reason
                .as_ref()
                .unwrap()
                .contains("China")
        );

        assert_eq!(models[2].status, "restricted");
        assert_eq!(models[2].origin.as_deref(), Some("China"));

        // French model: approved
        assert_eq!(models[3].status, "available");
        assert_eq!(models[3].origin.as_deref(), Some("France"));
    }

    // rtmx:req REQ-LLM-045
    #[test]
    fn test_origin_filter_preserves_non_available_status() {
        use crate::model_origin::ModelOriginPolicy;

        let mut models = vec![ModelInfo {
            model_id: "qwen:7b".to_string(),
            status: "unauthorized".to_string(),
            input_rate: 0.0,
            output_rate: 0.0,
            origin: None,
            restriction_reason: None,
        }];

        let policy = ModelOriginPolicy::default();
        apply_origin_policy(&mut models, &policy);

        // Status stays "unauthorized", not overwritten to "restricted"
        assert_eq!(models[0].status, "unauthorized");
        assert_eq!(models[0].origin.as_deref(), Some("China"));
    }

    // --- REQ-LLM-046: Model switch rejects restricted models ---

    // rtmx:req REQ-LLM-046
    #[test]
    fn test_switch_rejects_restricted_model() {
        use crate::model_origin::ModelOriginPolicy;

        let policy = ModelOriginPolicy::default();

        let result = check_model_switch("qwen:7b", &policy);
        assert!(result.is_err());
        let reason = result.unwrap_err();
        assert!(reason.contains("restricted"));
        assert!(reason.contains("China"));
    }

    // rtmx:req REQ-LLM-046
    #[test]
    fn test_switch_allows_approved_model() {
        use crate::model_origin::ModelOriginPolicy;

        let policy = ModelOriginPolicy::default();
        assert!(check_model_switch("llama3:latest", &policy).is_ok());
        assert!(check_model_switch("mistral:7b", &policy).is_ok());
        assert!(check_model_switch("gemma4:latest", &policy).is_ok());
    }

    // rtmx:req REQ-LLM-046
    #[test]
    fn test_switch_rejects_unknown_model_by_default() {
        use crate::model_origin::ModelOriginPolicy;

        let policy = ModelOriginPolicy::default();
        let result = check_model_switch("some-novel-model", &policy);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown"));
    }

    // rtmx:req REQ-LLM-046
    #[test]
    fn test_switch_allows_unknown_with_flag() {
        use crate::model_origin::ModelOriginPolicy;

        let policy = ModelOriginPolicy::default().allow_unclassified();
        assert!(check_model_switch("some-novel-model", &policy).is_ok());
    }
}
