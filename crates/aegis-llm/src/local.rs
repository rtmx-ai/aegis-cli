//! Local OpenAI-compatible provider for air-gapped operation.
//!
//! Connects to any server that implements the OpenAI chat completions
//! API: Ollama, vLLM, llama.cpp, text-generation-inference, etc.
//! Zero network egress beyond the configured LOCAL_ENDPOINT.
//!
//! # Cold-start latency (REQ-LLM-028)
//!
//! The first prompt after Ollama loads a model pays a 10-15 second
//! load-into-RAM cost; subsequent prompts are typically sub-second.
//! Call [`LocalProvider::warmup`] after `/connect local` to absorb
//! that cost up front, and [`LocalProvider::is_warm`] to check
//! whether a re-warmup is needed after an idle period.
//!
//! For longer-running aegis sessions, set `OLLAMA_KEEP_ALIVE=-1` in the
//! Ollama server's environment to keep models resident in GPU/RAM
//! indefinitely. Default keep-alive is 5 minutes after the last
//! request; for vLLM and llama.cpp the model is always resident and
//! the warmup cost is one-time at server start.

use std::time::{Duration, Instant};

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;
use reqwest::Client;

use crate::capabilities::needs_tool_shim;
use crate::config::ProviderConfig;
use crate::sse::SseTokenStream;

/// Latency threshold below which [`LocalProvider::is_warm`] reports
/// that the model is resident and serving from hot cache.
const WARM_LATENCY_THRESHOLD: Duration = Duration::from_secs(3);

/// Provider that speaks the OpenAI chat completions API.
pub struct LocalProvider {
    client: Client,
    endpoint: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
}

impl LocalProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, DomainError> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
            .timeout(std::time::Duration::from_secs(config.read_timeout_secs))
            .build()
            .map_err(|e| DomainError::ProviderError {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        tracing::info!(
            provider = "local",
            model = %config.model,
            endpoint = %config.endpoint,
            "provider initialized"
        );

        Ok(Self {
            client,
            endpoint: config.endpoint.trim_end_matches('/').to_string(),
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
        })
    }

    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> serde_json::Value {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                        Role::System => "system",
                    },
                    "content": m.content,
                })
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
            if needs_tool_shim(&self.model) {
                // Model lacks native tool calling -- inject tool
                // descriptions into the system prompt instead of
                // sending the `tools` field (which would be rejected).
                let shim_prompt = build_toolshim_system_prompt(tools);
                let msgs_arr = body["messages"].as_array_mut().unwrap();
                msgs_arr.insert(
                    0,
                    serde_json::json!({
                        "role": "system",
                        "content": shim_prompt,
                    }),
                );
            } else {
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
        }

        body
    }

    /// Send a minimal completion request and return the elapsed time.
    ///
    /// Eliminates the 10-15 s cold-start surprise on the first real
    /// prompt by forcing Ollama (or the configured OpenAI-compatible
    /// server) to load the model into GPU/RAM.
    ///
    /// This sends `{"model": ..., "prompt": "Hi", "max_tokens": 1,
    /// "stream": false}` to `POST {endpoint}/chat/completions`. The
    /// response body is discarded; only the HTTP round-trip latency
    /// is reported.
    ///
    /// Errors:
    /// * [`DomainError::ProviderError`] if the request fails or the
    ///   server responds with a non-success HTTP status.
    pub async fn warmup(&self) -> Result<Duration, DomainError> {
        let url = format!("{}/chat/completions", self.endpoint);
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 1,
            "temperature": self.temperature,
            "stream": false,
        });

        tracing::info!(
            model = %self.model,
            endpoint = %self.endpoint,
            "warming up local model"
        );

        let start = Instant::now();
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DomainError::ProviderError {
                message: format!("Warmup request to {url} failed: {e}"),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::ProviderError {
                message: format!("Warmup returned {status}: {body}"),
            });
        }

        // Drain the body so the connection can be reused; we do not
        // need the content.
        let _ = response.bytes().await;
        let elapsed = start.elapsed();

        tracing::info!(
            model = %self.model,
            latency_ms = elapsed.as_millis() as u64,
            "local model warmup complete"
        );

        Ok(elapsed)
    }

    /// Estimate whether the model is currently resident and serving
    /// from hot cache. Sends a 1-token completion and returns `true`
    /// if the round-trip latency is below [`WARM_LATENCY_THRESHOLD`].
    ///
    /// Returns `false` on any transport or HTTP error so that the
    /// caller treats an unhealthy endpoint as cold (a subsequent
    /// [`LocalProvider::warmup`] call will surface the real error).
    pub async fn is_warm(&self) -> bool {
        match self.warmup().await {
            Ok(elapsed) => elapsed < WARM_LATENCY_THRESHOLD,
            Err(_) => false,
        }
    }
}

/// Build a system prompt that describes available tools for models
/// without native tool/function calling support.
fn build_toolshim_system_prompt(tools: &[ToolSchema]) -> String {
    let mut prompt = String::from(
        "You have access to the following tools. To use a tool, respond with a JSON \
         object in the following format:\n\n\
         ```json\n{\"tool\": \"tool_name\", \"arguments\": {\"arg1\": \"value1\"}}\n```\n\n\
         If you do not need to use a tool, respond with plain text.\n\n\
         Available tools:\n",
    );

    for tool in tools {
        prompt.push_str(&format!(
            "\n- **{}**: {}\n  Parameters: {}\n",
            tool.name, tool.description, tool.parameters
        ));
    }

    prompt
}

#[async_trait]
impl LlmProvider for LocalProvider {
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

        let url = format!("{}/models", self.endpoint);
        let start = std::time::Instant::now();

        match health_client.get(&url).send().await {
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
        let url = format!("{}/chat/completions", self.endpoint);
        let body = self.build_request_body(messages, tools);
        tracing::debug!(
            model = %self.model,
            messages = messages.len(),
            tools = tools.len(),
            "starting LLM stream"
        );

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DomainError::ProviderError {
                message: format!("Request to {url} failed: {e}"),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::ProviderError {
                message: format!("LLM API returned {status}: {body}"),
            });
        }

        let bytes_stream = response.bytes_stream();
        Ok(Box::new(SseTokenStream::new(bytes_stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::ToolCall;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sse_chunk(content: &str) -> String {
        format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}\n\n")
    }

    fn sse_done() -> String {
        "data: [DONE]\n\n".to_string()
    }

    fn sse_usage(input: u64, output: u64) -> String {
        format!(
            "data: {{\"choices\":[{{\"delta\":{{}}}}],\"usage\":{{\"prompt_tokens\":{input},\"completion_tokens\":{output}}}}}\n\n"
        )
    }

    // rtmx:req REQ-LLM-004
    #[tokio::test]
    async fn local_provider_streams_text_response() {
        let server = MockServer::start().await;

        let body = format!(
            "{}{}{}",
            sse_chunk("Hello"),
            sse_chunk(" world"),
            sse_done()
        );

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "test-model");
        let provider = LocalProvider::new(&cfg).unwrap();

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
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

    // rtmx:req REQ-LLM-004
    #[tokio::test]
    async fn local_provider_handles_tool_calls() {
        let server = MockServer::start().await;

        let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"src/main.rs\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n".to_string();

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "test-model");
        let provider = LocalProvider::new(&cfg).unwrap();

        let messages = vec![Message {
            role: Role::User,
            content: "Read main.rs".to_string(),
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

    // rtmx:req REQ-LLM-004
    #[tokio::test]
    async fn local_provider_surfaces_http_errors() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "test-model");
        let provider = LocalProvider::new(&cfg).unwrap();

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
        }];

        let result = provider.stream(&messages, &[]).await;
        match result {
            Err(e) => {
                let err = e.to_string();
                assert!(
                    err.contains("500"),
                    "Error should contain status code: {err}"
                );
            }
            Ok(_) => panic!("Expected error for 500 response"),
        }
    }

    // rtmx:req REQ-LLM-004
    #[tokio::test]
    async fn local_provider_tracks_usage() {
        let server = MockServer::start().await;

        let body = format!("{}{}{}", sse_chunk("Hi"), sse_usage(10, 5), sse_done());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "test-model");
        let provider = LocalProvider::new(&cfg).unwrap();

        let messages = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
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
        assert_eq!(input, 10);
        assert_eq!(output, 5);
    }

    // rtmx:req REQ-LLM-001
    #[test]
    fn request_body_includes_model_and_messages() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

        let messages = vec![
            Message {
                role: Role::System,
                content: "You are helpful.".to_string(),
            },
            Message {
                role: Role::User,
                content: "Hello".to_string(),
            },
        ];

        let body = provider.build_request_body(&messages, &[]);

        assert_eq!(body["model"], "llama3");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        // No tools field when empty
        assert!(body.get("tools").is_none());
    }

    // rtmx:req REQ-LLM-001
    #[test]
    fn request_body_includes_tools_for_capable_model() {
        // gemini-2.5 supports native tool calling
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "gemini-2.5-pro");
        let provider = LocalProvider::new(&cfg).unwrap();

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
        }];

        let body = provider.build_request_body(&messages, &tools);

        let tools_arr = body["tools"].as_array().unwrap();
        assert_eq!(tools_arr.len(), 1);
        assert_eq!(tools_arr[0]["function"]["name"], "read_file");
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn request_body_uses_shim_for_llama3() {
        // llama3 does not support native tool calling -- tools should
        // be injected as a system prompt, not in the `tools` field.
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

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
        }];

        let body = provider.build_request_body(&messages, &tools);

        // tools field should NOT be present
        assert!(
            body.get("tools").is_none(),
            "llama3 should not have a tools field"
        );

        // A system message with tool descriptions should be injected
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        let sys_content = msgs[0]["content"].as_str().unwrap();
        assert!(
            sys_content.contains("read_file"),
            "Shim prompt should describe available tools"
        );
    }

    // rtmx:req REQ-LLM-016
    #[test]
    fn endpoint_url_constructed_correctly() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1/", "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();
        // Trailing slash should be stripped
        assert_eq!(provider.endpoint, "http://localhost:11434/v1");
    }

    // rtmx:req REQ-LLM-005
    #[tokio::test]
    async fn health_check_returns_healthy_for_responsive_endpoint() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"data":[{"id":"llama3"}]}"#),
            )
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

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
        let cfg = ProviderConfig::local("http://127.0.0.1:1/v1", "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

        let health = provider.health_check().await;
        match health {
            ProviderHealth::Unhealthy { message } => {
                assert!(!message.is_empty(), "Unhealthy message should not be empty");
            }
            other => panic!("Expected Unhealthy, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-005
    #[tokio::test]
    async fn health_check_returns_unhealthy_for_http_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

        let health = provider.health_check().await;
        match health {
            ProviderHealth::Unhealthy { message } => {
                assert!(
                    message.contains("500"),
                    "Should mention status code: {message}"
                );
            }
            other => panic!("Expected Unhealthy, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-005
    #[tokio::test]
    async fn health_check_returns_degraded_for_slow_endpoint() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"data":[]}"#)
                    .set_delay(std::time::Duration::from_millis(1500)),
            )
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

        let health = provider.health_check().await;
        match health {
            ProviderHealth::Degraded {
                latency_ms,
                message,
            } => {
                assert!(
                    latency_ms >= 1000,
                    "Expected latency >= 1000ms, got {latency_ms}ms"
                );
                assert!(!message.is_empty(), "Degraded message should not be empty");
            }
            other => panic!("Expected Degraded, got {:?}", other),
        }
    }

    // -- REQ-LLM-028: cold-start warmup --

    /// Minimal non-streaming chat-completions body for warmup mocks.
    fn warmup_response_body() -> &'static str {
        r#"{"id":"cmpl-1","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"."},"finish_reason":"length"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#
    }

    // rtmx:req REQ-LLM-028
    #[tokio::test]
    async fn test_warmup_ping_loads_model() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(warmup_response_body())
                    .insert_header("content-type", "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

        let elapsed = provider
            .warmup()
            .await
            .expect("warmup should succeed against mock");
        // Elapsed is always >= 0; the meaningful assertion is that the
        // mock was hit exactly once, which wiremock enforces on drop.
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "warmup latency should be measurable, got {elapsed:?}"
        );
    }

    // rtmx:req REQ-LLM-028
    #[tokio::test]
    async fn test_warmup_sends_minimal_request() {
        let server = MockServer::start().await;

        // Validate the warmup request shape: single "Hi" user message,
        // max_tokens=1, stream=false.
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(warmup_response_body()))
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();
        provider.warmup().await.expect("warmup should succeed");

        // Inspect the recorded request.
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1, "expected exactly one warmup request");
        let body: serde_json::Value =
            serde_json::from_slice(&received[0].body).expect("warmup body should be JSON");
        assert_eq!(body["model"], "llama3");
        assert_eq!(body["max_tokens"], 1);
        assert_eq!(body["stream"], false);
        let msgs = body["messages"].as_array().expect("messages array");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "Hi");
    }

    // rtmx:req REQ-LLM-028
    #[tokio::test]
    async fn test_warmup_surfaces_http_errors() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503).set_body_string("model loading"))
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

        let err = provider.warmup().await.expect_err("503 should error");
        let msg = err.to_string();
        assert!(
            msg.contains("503"),
            "error should mention status code: {msg}"
        );
    }

    // rtmx:req REQ-LLM-028
    #[tokio::test]
    async fn test_is_warm_returns_true_for_fast_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(warmup_response_body())
                    .set_delay(std::time::Duration::from_millis(50)),
            )
            .mount(&server)
            .await;

        let cfg = ProviderConfig::local(&format!("{}/v1", server.uri()), "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();

        assert!(
            provider.is_warm().await,
            "a sub-100ms response should report warm"
        );
    }

    // rtmx:req REQ-LLM-028
    #[tokio::test]
    async fn test_is_warm_returns_false_for_slow_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(warmup_response_body())
                    .set_delay(std::time::Duration::from_secs(4)),
            )
            .mount(&server)
            .await;

        let cfg = ProviderConfig {
            kind: crate::config::ProviderKind::Local,
            model: "llama3".to_string(),
            endpoint: format!("{}/v1", server.uri()),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            // Must exceed the 4 s server delay or the request times out
            // before we can observe the latency.
            read_timeout_secs: 30,
            project_id: None,
            region: None,
        };
        let provider = LocalProvider::new(&cfg).unwrap();

        assert!(
            !provider.is_warm().await,
            "a 4s response should report NOT warm"
        );
    }

    // rtmx:req REQ-LLM-028
    #[tokio::test]
    async fn test_is_warm_returns_false_for_unreachable_endpoint() {
        let cfg = ProviderConfig::local("http://127.0.0.1:1/v1", "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();
        assert!(
            !provider.is_warm().await,
            "unreachable endpoint must not be considered warm"
        );
    }
}
