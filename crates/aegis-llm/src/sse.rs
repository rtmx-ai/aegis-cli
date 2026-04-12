//! Shared SSE (Server-Sent Events) parser for OpenAI-compatible streaming APIs.
//!
//! Extracts token streams from the `data: {...}` lines emitted by
//! OpenAI, Ollama, vLLM, and Vertex AI (Gemini OpenAI-compat mode).
//! Both `LocalProvider` and `VertexProvider` reuse this parser.

use aegis_domain::ports::*;
use aegis_domain::types::*;
use async_trait::async_trait;
use serde::Deserialize;

/// Parses Server-Sent Events from the OpenAI streaming format.
pub struct SseTokenStream {
    buffer: String,
    done: bool,
    input_tokens: u64,
    output_tokens: u64,
    inner: Box<dyn futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin>,
}

impl SseTokenStream {
    pub fn new<S>(stream: S) -> Self
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

pub fn parse_tool_call(name: &str, args_json: &str) -> Option<StreamEvent> {
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
pub(crate) struct SseChunk {
    pub choices: Vec<SseChoice>,
    pub usage: Option<SseUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseChoice {
    pub delta: SseDelta,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseDelta {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<SseToolCall>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseToolCall {
    pub function: Option<SseFunction>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SseUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}
