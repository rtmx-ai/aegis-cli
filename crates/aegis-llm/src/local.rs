//! Local OpenAI-compatible provider for air-gapped operation.
//!
//! Connects to any server that implements the OpenAI chat completions
//! API: Ollama, vLLM, llama.cpp, text-generation-inference, etc.
//! Zero network egress beyond the configured LOCAL_ENDPOINT.

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use aegis_domain::types::*;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::config::ProviderConfig;

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
impl LlmProvider for LocalProvider {
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

/// Parses Server-Sent Events from the OpenAI streaming format.
pub struct SseTokenStream {
    buffer: String,
    done: bool,
    input_tokens: u64,
    output_tokens: u64,
    inner: Box<dyn futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin>,
}

impl SseTokenStream {
    fn new<S>(stream: S) -> Self
    where
        S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
    {
        Self {
            buffer: String::new(),
            done: false,
            input_tokens: 0,
            output_tokens: 0,
            inner: Box::new(stream),
        }
    }

    fn parse_sse_line(&mut self, line: &str) -> Option<StreamEvent> {
        let data = line.strip_prefix("data: ")?;

        if data == "[DONE]" {
            self.done = true;
            return Some(StreamEvent::Done {
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
            });
        }

        let chunk: SseChunk = serde_json::from_str(data).ok()?;

        // Track usage if present
        if let Some(usage) = &chunk.usage {
            self.input_tokens = usage.prompt_tokens;
            self.output_tokens = usage.completion_tokens;
        }

        let choice = chunk.choices.first()?;

        // Check for tool calls
        if let Some(SseToolCall {
            function: Some(func),
        }) = choice.delta.tool_calls.as_deref().and_then(|tc| tc.first())
        {
            let name = func.name.clone().unwrap_or_default();
            let args = func.arguments.clone().unwrap_or_default();
            if !name.is_empty() {
                return parse_tool_call(&name, &args);
            }
        }

        // Regular text content
        let content = choice.delta.content.as_deref()?;
        if !content.is_empty() {
            self.output_tokens += 1; // approximate
            Some(StreamEvent::Token(content.to_string()))
        } else {
            None
        }
    }
}

fn parse_tool_call(name: &str, args_json: &str) -> Option<StreamEvent> {
    let args: serde_json::Value = serde_json::from_str(args_json).ok()?;
    let tool_call = match name {
        "read_file" => ToolCall::ReadFile {
            path: FilePath::new_unchecked(args["path"].as_str()?),
        },
        "write_file" => ToolCall::WriteFile {
            path: FilePath::new_unchecked(args["path"].as_str()?),
            content: args["content"].as_str()?.to_string(),
        },
        "run_command" => ToolCall::RunCommand {
            command: args["command"].as_str()?.to_string(),
            timeout_secs: args["timeout"].as_u64().unwrap_or(60),
        },
        "list_dir" => ToolCall::ListDir {
            path: FilePath::new_unchecked(args["path"].as_str()?),
        },
        "grep" => ToolCall::Grep {
            pattern: args["pattern"].as_str()?.to_string(),
            path: FilePath::new_unchecked(args["path"].as_str()?),
        },
        _ => return None,
    };
    Some(StreamEvent::ToolUse(tool_call))
}

#[async_trait]
impl TokenStream for SseTokenStream {
    async fn next(&mut self) -> Option<StreamEvent> {
        use futures::StreamExt;

        loop {
            if self.done {
                return None;
            }

            // Check buffer for complete lines
            while let Some(newline_pos) = self.buffer.find('\n') {
                let line: String = self.buffer.drain(..=newline_pos).collect();
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(event) = self.parse_sse_line(line) {
                    return Some(event);
                }
            }

            // Read more data from the stream
            match self.inner.next().await {
                Some(Ok(bytes)) => {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        self.buffer.push_str(text);
                    }
                }
                Some(Err(e)) => {
                    return Some(StreamEvent::Error(format!("Stream error: {e}")));
                }
                None => {
                    // Stream ended without [DONE]
                    if !self.done {
                        self.done = true;
                        return Some(StreamEvent::Done {
                            input_tokens: self.input_tokens,
                            output_tokens: self.output_tokens,
                        });
                    }
                    return None;
                }
            }
        }
    }
}

/// OpenAI SSE chunk schema.
#[derive(Debug, Deserialize)]
struct SseChunk {
    choices: Vec<SseChoice>,
    usage: Option<SseUsage>,
}

#[derive(Debug, Deserialize)]
struct SseChoice {
    delta: SseDelta,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    content: Option<String>,
    tool_calls: Option<Vec<SseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct SseToolCall {
    function: Option<SseFunction>,
}

#[derive(Debug, Deserialize)]
struct SseFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // @req REQ-LLM-004
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

    // @req REQ-LLM-004
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

    // @req REQ-LLM-004
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

    // @req REQ-LLM-004
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

    // @req REQ-LLM-001
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

    // @req REQ-LLM-001
    #[test]
    fn request_body_includes_tools_when_provided() {
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

        let tools_arr = body["tools"].as_array().unwrap();
        assert_eq!(tools_arr.len(), 1);
        assert_eq!(tools_arr[0]["function"]["name"], "read_file");
    }

    // @req REQ-LLM-016
    #[test]
    fn endpoint_url_constructed_correctly() {
        let cfg = ProviderConfig::local("http://localhost:11434/v1/", "llama3");
        let provider = LocalProvider::new(&cfg).unwrap();
        // Trailing slash should be stripped
        assert_eq!(provider.endpoint, "http://localhost:11434/v1");
    }
}
