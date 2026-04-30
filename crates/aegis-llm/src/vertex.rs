//! Vertex AI (Gemini) provider via the OpenAI-compatible endpoint.
//!
//! Connects to Google Cloud's Vertex AI using the OpenAI chat completions
//! compatible API. Requires a GCP project ID, region, and a valid OAuth2
//! access token obtained via Application Default Credentials (ADC).

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;
use reqwest::Client;

use crate::config::ProviderConfig;
use crate::sse::SseTokenStream;

/// Provider that speaks to Vertex AI via the OpenAI-compatible endpoint.
#[derive(Debug)]
pub struct VertexProvider {
    client: Client,
    endpoint_url: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
    access_token: String,
}

impl VertexProvider {
    /// Create a new VertexProvider from config and a pre-resolved access token.
    ///
    /// Requires `config.project_id` and `config.region` to be `Some`.
    pub fn new(config: &ProviderConfig, access_token: String) -> Result<Self, DomainError> {
        let project_id =
            config
                .project_id
                .as_deref()
                .ok_or_else(|| DomainError::ConfigError {
                    message: "Vertex AI provider requires project_id in config".to_string(),
                })?;

        let region = config
            .region
            .as_deref()
            .ok_or_else(|| DomainError::ConfigError {
                message: "Vertex AI provider requires region in config".to_string(),
            })?;

        let endpoint_url = format!(
            "https://{region}-aiplatform.googleapis.com/v1/projects/{project_id}\
             /locations/{region}/endpoints/openapi/chat/completions"
        );

        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
            .timeout(std::time::Duration::from_secs(config.read_timeout_secs))
            .build()
            .map_err(|e| DomainError::ProviderError {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        tracing::info!(
            provider = "vertex",
            model = %config.model,
            endpoint = %endpoint_url,
            project_id = %project_id,
            region = %region,
            "provider initialized"
        );

        Ok(Self {
            client,
            endpoint_url,
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            access_token,
        })
    }

    /// Build the OpenAI-compatible request body (same format as LocalProvider).
    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> serde_json::Value {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                let mut msg = serde_json::json!({
                    "role": match m.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                        Role::System => "system",
                    },
                    "content": m.content,
                });
                if let Some(ref cc) = m.cache_control {
                    msg["cache_control"] = serde_json::json!({"type": cc});
                }
                msg
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": msgs,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
            "stream": true,
        });

        if !tools.is_empty() {
            let tool_defs: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(tool_defs);
        }

        body
    }
}

#[async_trait]
impl LlmProvider for VertexProvider {
    async fn health_check(&self) -> ProviderHealth {
        let health_client = match reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return ProviderHealth::Unhealthy {
                    message: format!("Failed to create health check client: {e}"),
                };
            }
        };

        // Extract base URL from endpoint_url for the models list endpoint.
        // endpoint_url is like:
        //   https://{region}-aiplatform.googleapis.com/v1/projects/{project}/...
        // We probe the models endpoint at the same base.
        let models_url = if let Some(pos) = self.endpoint_url.find("/v1/") {
            format!("{}/v1/models", &self.endpoint_url[..pos])
        } else {
            format!("{}/models", self.endpoint_url)
        };

        let start = std::time::Instant::now();

        match health_client
            .get(&models_url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await
        {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                if !response.status().is_success() {
                    return ProviderHealth::Unhealthy {
                        message: format!("Health check returned HTTP {}", response.status()),
                    };
                }
                if latency_ms < 1000 {
                    ProviderHealth::Healthy { latency_ms }
                } else {
                    ProviderHealth::Degraded {
                        latency_ms,
                        message: format!("Response took {latency_ms}ms (> 1000ms threshold)"),
                    }
                }
            }
            Err(e) => ProviderHealth::Unhealthy {
                message: format!("Health check failed: {e}"),
            },
        }
    }

    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        let body = self.build_request_body(messages, tools);
        tracing::debug!(
            model = %self.model,
            messages = messages.len(),
            tools = tools.len(),
            "starting Vertex AI LLM stream"
        );

        let response = self
            .client
            .post(&self.endpoint_url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .json(&body)
            .send()
            .await
            .map_err(|e| DomainError::ProviderError {
                message: format!("Request to {} failed: {e}", self.endpoint_url),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::ProviderError {
                message: format!("Vertex AI API returned {status}: {body}"),
            });
        }

        let bytes_stream = response.bytes_stream();
        Ok(Box::new(SseTokenStream::new(bytes_stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ProviderKind};
    use aegis_domain::types::ToolCall;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn vertex_config(project_id: Option<&str>, region: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::Vertex,
            model: "gemini-2.5-pro-001".to_string(),
            endpoint: "https://vertex.googleapis.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: project_id.map(|s| s.to_string()),
            region: region.map(|s| s.to_string()),
        }
    }

    // rtmx:req REQ-LLM-020
    #[test]
    fn new_fails_without_project_id() {
        let cfg = vertex_config(None, Some("us-central1"));
        let result = VertexProvider::new(&cfg, "ya29.test".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("project_id"),
            "Error should mention project_id: {err}"
        );
    }

    // rtmx:req REQ-LLM-020
    #[test]
    fn new_fails_without_region() {
        let cfg = vertex_config(Some("my-project"), None);
        let result = VertexProvider::new(&cfg, "ya29.test".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("region"), "Error should mention region: {err}");
    }

    // rtmx:req REQ-LLM-020
    #[test]
    fn new_builds_correct_endpoint_url() {
        let cfg = vertex_config(Some("my-project-123"), Some("us-central1"));
        let provider = VertexProvider::new(&cfg, "ya29.test".to_string()).unwrap();
        assert_eq!(
            provider.endpoint_url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project-123\
             /locations/us-central1/endpoints/openapi/chat/completions"
        );
    }

    // rtmx:req REQ-LLM-020
    #[test]
    fn new_stores_model_and_tokens() {
        let cfg = vertex_config(Some("proj"), Some("us-east4"));
        let provider = VertexProvider::new(&cfg, "ya29.tok".to_string()).unwrap();
        assert_eq!(provider.model, "gemini-2.5-pro-001");
        assert_eq!(provider.max_tokens, 4096);
        assert_eq!(provider.temperature, 0.0);
        assert_eq!(provider.access_token, "ya29.tok");
    }

    // rtmx:req REQ-LLM-020
    #[test]
    fn request_body_includes_model_and_messages() {
        let cfg = vertex_config(Some("proj"), Some("us-east4"));
        let provider = VertexProvider::new(&cfg, "ya29.tok".to_string()).unwrap();

        let messages = vec![
            Message {
                role: Role::System,
                content: "You are helpful.".to_string(),
                cache_control: None,
            },
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                cache_control: None,
            },
        ];

        let body = provider.build_request_body(&messages, &[]);

        assert_eq!(body["model"], "gemini-2.5-pro-001");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body.get("tools").is_none());
    }

    // rtmx:req REQ-LLM-020
    #[test]
    fn request_body_includes_tools_when_provided() {
        let cfg = vertex_config(Some("proj"), Some("us-east4"));
        let provider = VertexProvider::new(&cfg, "ya29.tok".to_string()).unwrap();

        let tools = vec![ToolSchema {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        }];

        let messages = vec![Message {
            role: Role::User,
            content: "Read foo.rs".to_string(),
            cache_control: None,
        }];

        let body = provider.build_request_body(&messages, &tools);

        let tools_arr = body["tools"].as_array().unwrap();
        assert_eq!(tools_arr.len(), 1);
        assert_eq!(tools_arr[0]["function"]["name"], "read_file");
    }

    fn sse_chunk(content: &str) -> String {
        format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}\n\n")
    }

    fn sse_done() -> String {
        "data: [DONE]\n\n".to_string()
    }

    fn sse_usage(input: u64, output: u64) -> String {
        format!(
            "data: {{\"choices\":[{{\"delta\":{{}}}}],\"usage\":\
             {{\"prompt_tokens\":{input},\"completion_tokens\":{output}}}}}\n\n"
        )
    }

    // rtmx:req REQ-LLM-020
    #[tokio::test]
    async fn vertex_provider_streams_text_response() {
        let server = MockServer::start().await;

        let body = format!(
            "{}{}{}",
            sse_chunk("Hello"),
            sse_chunk(" world"),
            sse_done()
        );

        Mock::given(method("POST"))
            .and(header("Authorization", "Bearer ya29.test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        // Override the endpoint_url to point at our mock server
        let cfg = vertex_config(Some("proj"), Some("us-central1"));
        let mut provider = VertexProvider::new(&cfg, "ya29.test-token".to_string()).unwrap();
        provider.endpoint_url = format!("{}/chat/completions", server.uri());

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
            cache_control: None,
        }];

        let mut stream = provider.stream(&messages, &[]).await.unwrap();

        let mut tokens = Vec::new();
        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::Token(t) => tokens.push(t),
                StreamEvent::Done { .. } => break,
                _ => {}
            }
        }

        assert_eq!(tokens, vec!["Hello", " world"]);
    }

    // rtmx:req REQ-LLM-020
    #[tokio::test]
    async fn vertex_provider_handles_tool_calls() {
        let server = MockServer::start().await;

        let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":\
             {\"name\":\"read_file\",\"arguments\":\
             \"{\\\"path\\\":\\\"src/main.rs\\\"}\"}}]}}]}\n\n\
             data: [DONE]\n\n"
            .to_string();

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let cfg = vertex_config(Some("proj"), Some("us-central1"));
        let mut provider = VertexProvider::new(&cfg, "ya29.test-token".to_string()).unwrap();
        provider.endpoint_url = format!("{}/chat/completions", server.uri());

        let messages = vec![Message {
            role: Role::User,
            content: "Read main.rs".to_string(),
            cache_control: None,
        }];

        let mut stream = provider.stream(&messages, &[]).await.unwrap();

        let mut got_tool_use = false;
        while let Some(event) = stream.next().await {
            if let StreamEvent::ToolUse(ToolCall::ReadFile { path }) = event {
                assert_eq!(path.as_path().to_str().unwrap(), "src/main.rs");
                got_tool_use = true;
            }
        }
        assert!(got_tool_use, "Expected a ToolUse event");
    }

    // rtmx:req REQ-LLM-020
    #[tokio::test]
    async fn vertex_provider_surfaces_http_errors() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let cfg = vertex_config(Some("proj"), Some("us-central1"));
        let mut provider = VertexProvider::new(&cfg, "ya29.expired".to_string()).unwrap();
        provider.endpoint_url = format!("{}/chat/completions", server.uri());

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
            cache_control: None,
        }];

        let result = provider.stream(&messages, &[]).await;
        match result {
            Err(e) => {
                let err = e.to_string();
                assert!(
                    err.contains("401"),
                    "Error should contain status code: {err}"
                );
            }
            Ok(_) => panic!("Expected error for 401 response"),
        }
    }

    // rtmx:req REQ-LLM-020
    #[tokio::test]
    async fn vertex_provider_tracks_usage() {
        let server = MockServer::start().await;

        let body = format!("{}{}{}", sse_chunk("Hi"), sse_usage(15, 8), sse_done());

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let cfg = vertex_config(Some("proj"), Some("us-central1"));
        let mut provider = VertexProvider::new(&cfg, "ya29.test".to_string()).unwrap();
        provider.endpoint_url = format!("{}/chat/completions", server.uri());

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
            cache_control: None,
        }];

        let mut stream = provider.stream(&messages, &[]).await.unwrap();

        let mut done_event = None;
        while let Some(event) = stream.next().await {
            if let StreamEvent::Done {
                input_tokens,
                output_tokens,
            } = event
            {
                done_event = Some((input_tokens, output_tokens));
            }
        }

        let (input, output) = done_event.expect("Should have received Done event");
        assert_eq!(input, 15);
        assert_eq!(output, 8);
    }

    // rtmx:req REQ-LLM-020
    #[test]
    fn endpoint_url_uses_correct_region_and_project() {
        let cfg = vertex_config(Some("aegis-il4-prod"), Some("us-east4"));
        let provider = VertexProvider::new(&cfg, "ya29.tok".to_string()).unwrap();
        assert!(
            provider
                .endpoint_url
                .starts_with("https://us-east4-aiplatform")
        );
        assert!(provider.endpoint_url.contains("aegis-il4-prod"));
        assert!(provider.endpoint_url.contains("/locations/us-east4/"));
    }

    // rtmx:req REQ-LLM-005
    #[tokio::test]
    async fn health_check_returns_healthy_for_responsive_endpoint() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(header("Authorization", "Bearer ya29.health"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"models":[]}"#))
            .mount(&server)
            .await;

        let cfg = vertex_config(Some("proj"), Some("us-central1"));
        let mut provider = VertexProvider::new(&cfg, "ya29.health".to_string()).unwrap();
        // Override endpoint to point at mock server with /v1/ path
        provider.endpoint_url = format!(
            "{}/v1/projects/proj/locations/us-central1/chat",
            server.uri()
        );

        let health = provider.health_check().await;
        match health {
            ProviderHealth::Healthy { latency_ms } => {
                assert!(
                    latency_ms < 1000,
                    "Expected latency < 1000ms, got {latency_ms}ms"
                );
            }
            other => panic!("Expected Healthy, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-005
    #[tokio::test]
    async fn health_check_returns_unhealthy_for_unreachable_endpoint() {
        let cfg = vertex_config(Some("proj"), Some("us-central1"));
        let mut provider = VertexProvider::new(&cfg, "ya29.tok".to_string()).unwrap();
        provider.endpoint_url =
            "http://127.0.0.1:1/v1/projects/proj/locations/us-central1/chat".to_string();

        let health = provider.health_check().await;
        match health {
            ProviderHealth::Unhealthy { message } => {
                assert!(!message.is_empty(), "Unhealthy message should not be empty");
            }
            other => panic!("Expected Unhealthy, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn request_body_includes_cache_control_when_set() {
        let cfg = vertex_config(Some("proj"), Some("us-east4"));
        let provider = VertexProvider::new(&cfg, "ya29.tok".to_string()).unwrap();

        let messages = vec![Message {
            role: Role::System,
            content: "You are helpful.".to_string(),
            cache_control: Some("ephemeral".to_string()),
        }];

        let body = provider.build_request_body(&messages, &[]);
        let msg = &body["messages"][0];
        assert_eq!(msg["cache_control"]["type"], "ephemeral");
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn request_body_omits_cache_control_when_none() {
        let cfg = vertex_config(Some("proj"), Some("us-east4"));
        let provider = VertexProvider::new(&cfg, "ya29.tok".to_string()).unwrap();

        let messages = vec![Message {
            role: Role::System,
            content: "You are helpful.".to_string(),
            cache_control: None,
        }];

        let body = provider.build_request_body(&messages, &[]);
        let msg = &body["messages"][0];
        assert!(
            msg.get("cache_control").is_none(),
            "cache_control should be absent when None"
        );
    }
}
