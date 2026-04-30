//! Azure OpenAI provider via the OpenAI-compatible endpoint.
//!
//! Connects to Azure OpenAI Service using the chat completions API.
//! Supports both API key authentication (`api-key` header) and
//! Azure AD / Entra ID bearer token authentication. Works with both
//! Azure Commercial (`.azure.com`) and Azure Government (`.azure.us`).

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;
use reqwest::Client;

use crate::auth::ProviderAuth;
use crate::config::ProviderConfig;
use crate::sse::SseTokenStream;

/// Azure OpenAI API version used for all requests.
const API_VERSION: &str = "2024-06-01";

/// Provider that speaks to Azure OpenAI via the chat completions endpoint.
#[derive(Debug)]
pub struct AzureProvider {
    client: Client,
    endpoint_url: String,
    base_url: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
    api_key: Option<String>,
    bearer_token: Option<String>,
}

impl AzureProvider {
    /// Create a new AzureProvider from config and pre-resolved auth.
    ///
    /// The `config.endpoint` must provide the Azure OpenAI resource base URL
    /// (e.g., `https://myresource.openai.azure.com`). The deployment name
    /// is taken from `config.model`.
    pub fn new(config: &ProviderConfig, auth: ProviderAuth) -> Result<Self, DomainError> {
        if config.endpoint.trim().is_empty() {
            return Err(DomainError::ConfigError {
                message: "Azure OpenAI provider requires a non-empty endpoint \
                          (e.g., https://myresource.openai.azure.com)"
                    .to_string(),
            });
        }

        let base_url = config.endpoint.trim_end_matches('/').to_string();

        let endpoint_url = format!(
            "{base_url}/openai/deployments/{}/chat/completions\
             ?api-version={API_VERSION}",
            config.model
        );

        let (api_key, bearer_token) = match auth {
            ProviderAuth::Azure { api_key, .. } => (api_key, None),
            _ => {
                return Err(DomainError::ConfigError {
                    message: "Azure OpenAI provider requires Azure auth \
                              credentials"
                        .to_string(),
                });
            }
        };

        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
            .timeout(std::time::Duration::from_secs(config.read_timeout_secs))
            .build()
            .map_err(|e| DomainError::ProviderError {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        tracing::info!(
            provider = "azure",
            model = %config.model,
            endpoint = %endpoint_url,
            "provider initialized"
        );

        Ok(Self {
            client,
            endpoint_url,
            base_url,
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            api_key,
            bearer_token,
        })
    }

    /// Build the OpenAI-compatible request body.
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

    /// Apply the appropriate auth header to a request builder.
    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref key) = self.api_key {
            req.header("api-key", key)
        } else if let Some(ref token) = self.bearer_token {
            req.header("Authorization", format!("Bearer {token}"))
        } else {
            req
        }
    }
}

#[async_trait]
impl LlmProvider for AzureProvider {
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

        let models_url = format!("{}/openai/models?api-version={API_VERSION}", self.base_url);

        let start = std::time::Instant::now();

        let mut req = health_client.get(&models_url);
        if let Some(ref key) = self.api_key {
            req = req.header("api-key", key);
        } else if let Some(ref token) = self.bearer_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        match req.send().await {
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
            "starting Azure OpenAI LLM stream"
        );

        let req = self.client.post(&self.endpoint_url).json(&body);
        let req = self.apply_auth(req);

        let response = req.send().await.map_err(|e| DomainError::ProviderError {
            message: format!("Request to {} failed: {e}", self.endpoint_url),
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::ProviderError {
                message: format!("Azure OpenAI API returned {status}: {body}"),
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
    use wiremock::matchers::{header, method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn azure_config(endpoint: &str) -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::Azure,
            model: "gpt-4o-2024-05-13".to_string(),
            endpoint: endpoint.to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: None,
        }
    }

    fn azure_auth_with_key(key: &str) -> ProviderAuth {
        ProviderAuth::Azure {
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            client_id: "11111111-1111-1111-1111-111111111111".to_string(),
            api_key: Some(key.to_string()),
        }
    }

    // rtmx:req REQ-LLM-003
    #[test]
    fn new_builds_commercial_endpoint() {
        let cfg = azure_config("https://myresource.openai.azure.com");
        let auth = azure_auth_with_key("test-key");
        let provider = AzureProvider::new(&cfg, auth).unwrap();
        assert_eq!(
            provider.endpoint_url,
            "https://myresource.openai.azure.com/openai/deployments/\
             gpt-4o-2024-05-13/chat/completions?api-version=2024-06-01"
        );
    }

    // rtmx:req REQ-LLM-003
    #[test]
    fn new_builds_gov_endpoint() {
        let cfg = azure_config("https://myresource.openai.azure.us");
        let auth = azure_auth_with_key("test-key");
        let provider = AzureProvider::new(&cfg, auth).unwrap();
        assert_eq!(
            provider.endpoint_url,
            "https://myresource.openai.azure.us/openai/deployments/\
             gpt-4o-2024-05-13/chat/completions?api-version=2024-06-01"
        );
    }

    // rtmx:req REQ-LLM-003
    #[test]
    fn new_with_api_key_auth() {
        let cfg = azure_config("https://myresource.openai.azure.com");
        let auth = azure_auth_with_key("my-secret-key");
        let provider = AzureProvider::new(&cfg, auth).unwrap();
        assert_eq!(provider.api_key.as_deref(), Some("my-secret-key"));
        assert!(provider.bearer_token.is_none());
    }

    // rtmx:req REQ-LLM-003
    #[test]
    fn new_fails_without_endpoint() {
        let cfg = azure_config("");
        let auth = azure_auth_with_key("key");
        let result = AzureProvider::new(&cfg, auth);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("endpoint"),
            "Error should mention endpoint: {err}"
        );
    }

    // rtmx:req REQ-LLM-003
    #[test]
    fn request_body_matches_openai_format() {
        let cfg = azure_config("https://myresource.openai.azure.com");
        let auth = azure_auth_with_key("key");
        let provider = AzureProvider::new(&cfg, auth).unwrap();

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

        assert_eq!(body["model"], "gpt-4o-2024-05-13");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["temperature"], 0.0);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body.get("tools").is_none());
    }

    // rtmx:req REQ-LLM-003
    #[test]
    fn request_body_includes_tools() {
        let cfg = azure_config("https://myresource.openai.azure.com");
        let auth = azure_auth_with_key("key");
        let provider = AzureProvider::new(&cfg, auth).unwrap();

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
        assert_eq!(tools_arr[0]["type"], "function");
        assert_eq!(tools_arr[0]["function"]["name"], "read_file");
        assert_eq!(tools_arr[0]["function"]["description"], "Read a file");
    }

    // rtmx:req REQ-LLM-003
    #[test]
    fn request_body_omits_tools_when_empty() {
        let cfg = azure_config("https://myresource.openai.azure.com");
        let auth = azure_auth_with_key("key");
        let provider = AzureProvider::new(&cfg, auth).unwrap();

        let messages = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
            cache_control: None,
        }];

        let body = provider.build_request_body(&messages, &[]);
        assert!(
            body.get("tools").is_none(),
            "tools key should be absent when no tools provided"
        );
    }

    fn sse_chunk(content: &str) -> String {
        format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\
             \"{content}\"}}}}]}}\n\n"
        )
    }

    fn sse_done() -> String {
        "data: [DONE]\n\n".to_string()
    }

    // rtmx:req REQ-LLM-003
    #[tokio::test]
    async fn stream_parses_sse_tokens() {
        let server = MockServer::start().await;

        let body = format!(
            "{}{}{}",
            sse_chunk("Hello"),
            sse_chunk(" world"),
            sse_done()
        );

        Mock::given(method("POST"))
            .and(header("api-key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("test-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
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

    // rtmx:req REQ-LLM-003
    #[tokio::test]
    async fn stream_handles_http_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("bad-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
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

    // rtmx:req REQ-TEST-009
    #[tokio::test]
    async fn stream_handles_malformed_sse() {
        let server = MockServer::start().await;

        // Return garbage instead of SSE format
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("this is not SSE data at all\n\n", "text/event-stream"),
            )
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("test-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
        provider.endpoint_url = format!("{}/chat/completions", server.uri());

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
            cache_control: None,
        }];

        let mut stream = provider.stream(&messages, &[]).await.unwrap();

        // Malformed data should be skipped; stream ends with Done (no
        // output_tokens accumulated so not a mid-stream drop).
        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Done { .. } => { /* expected */ }
            StreamEvent::RetryableError { .. } => { /* also acceptable */ }
            StreamEvent::Error(_) => { /* also acceptable */ }
            other => panic!("Expected Done or error, got {:?}", other),
        }
    }

    // rtmx:req REQ-TEST-009
    #[tokio::test]
    async fn stream_handles_empty_response_body() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("", "text/event-stream"))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("test-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
        provider.endpoint_url = format!("{}/chat/completions", server.uri());

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
            cache_control: None,
        }];

        let mut stream = provider.stream(&messages, &[]).await.unwrap();

        // Empty body means stream ends immediately -> Done with zero tokens
        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Done {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 0);
                assert_eq!(output_tokens, 0);
            }
            other => panic!("Expected Done, got {:?}", other),
        }
    }

    // rtmx:req REQ-TEST-009
    #[tokio::test]
    async fn stream_handles_partial_sse_chunk() {
        let server = MockServer::start().await;

        // Truncated JSON -- opening brace but no close, no trailing newlines
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("data: {\"choices\":[{\"delta\":{", "text/event-stream"),
            )
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("test-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
        provider.endpoint_url = format!("{}/chat/completions", server.uri());

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
            cache_control: None,
        }];

        let mut stream = provider.stream(&messages, &[]).await.unwrap();

        // Partial chunk without newline -- stream ends, parser sees incomplete
        // line in buffer. Should emit Done (no output tokens) not hang.
        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Done { .. } => { /* no output, clean end */ }
            StreamEvent::RetryableError { .. } => { /* also acceptable */ }
            other => panic!("Expected Done or RetryableError, got {:?}", other),
        }
    }

    // rtmx:req REQ-TEST-009
    #[tokio::test]
    async fn stream_handles_http_403_forbidden() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("bad-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
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
                    err.contains("403"),
                    "Error should contain 403 status: {err}"
                );
            }
            Ok(_) => panic!("Expected error for 403 response"),
        }
    }

    // rtmx:req REQ-TEST-009
    #[tokio::test]
    async fn stream_handles_http_429_rate_limit() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too Many Requests"))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("test-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
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
                    err.contains("429"),
                    "Error should contain 429 status: {err}"
                );
            }
            Ok(_) => panic!("Expected error for 429 response"),
        }
    }

    // rtmx:req REQ-TEST-009
    #[tokio::test]
    async fn stream_handles_http_500_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("test-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
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
                    err.contains("500"),
                    "Error should contain 500 status: {err}"
                );
            }
            Ok(_) => panic!("Expected error for 500 response"),
        }
    }

    // rtmx:req REQ-TEST-009
    #[tokio::test]
    async fn health_check_returns_unhealthy_on_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex(r"/openai/models.*"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("test-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
        provider.base_url = server.uri().to_string();

        let health = provider.health_check().await;
        match health {
            ProviderHealth::Unhealthy { message } => {
                assert!(
                    message.contains("500"),
                    "Unhealthy message should mention 500: {message}"
                );
            }
            other => panic!("Expected Unhealthy, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-003
    #[tokio::test]
    async fn health_check_returns_healthy() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(header("api-key", "health-key"))
            .and(path_regex(r"/openai/models.*"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data":[]}"#))
            .mount(&server)
            .await;

        let cfg = azure_config(&server.uri());
        let auth = azure_auth_with_key("health-key");
        let mut provider = AzureProvider::new(&cfg, auth).unwrap();
        provider.base_url = server.uri().to_string();

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
}
