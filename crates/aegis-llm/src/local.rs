//! Local OpenAI-compatible provider for air-gapped operation.
//!
//! Connects to any server that implements the OpenAI chat completions
//! API: Ollama, vLLM, llama.cpp, text-generation-inference, etc.
//! Zero network egress beyond the configured LOCAL_ENDPOINT.

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;
use reqwest::Client;

use crate::capabilities::needs_tool_shim;
use crate::config::ProviderConfig;
use crate::sse::SseTokenStream;

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
}
