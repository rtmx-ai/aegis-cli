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
                    let message = format!("Stream error: {e}");
                    let retryable = is_retryable_error(&message);
                    self.done = true;
                    return Some(StreamEvent::RetryableError { message, retryable });
                }
                None => {
                    // Stream ended without [DONE] -- connection dropped
                    if !self.done {
                        self.done = true;
                        // If we had partial output, this is a mid-stream drop
                        if self.output_tokens > 0 {
                            return Some(StreamEvent::RetryableError {
                                message: "Stream ended unexpectedly without \
                                          [DONE] marker (connection dropped)"
                                    .to_string(),
                                retryable: true,
                            });
                        }
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

/// Classify whether an error message represents a retryable failure (REQ-LLM-009).
///
/// Retryable errors include network issues (connection reset, timeout, DNS),
/// server-side failures (5xx), and rate limits (429). Non-retryable errors
/// include authentication (401, 403) and other client errors (4xx).
pub fn is_retryable_error(error: &str) -> bool {
    let lower = error.to_lowercase();

    // Network / transport errors
    if lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("dns")
        || lower.contains("eof")
        || lower.contains("network")
        || lower.contains("connect error")
    {
        return true;
    }

    // HTTP status codes: 429 and 5xx are retryable
    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
    {
        return true;
    }

    // 5xx server errors
    for code in ["500", "502", "503", "504"] {
        if lower.contains(code) {
            return true;
        }
    }

    // 4xx client errors are NOT retryable (401, 403, 400, 404, etc.)
    // Auth errors
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("400")
        || lower.contains("404")
        || lower.contains("422")
    {
        return false;
    }

    // Default: treat unknown stream errors as retryable (conservative)
    true
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_timeout() {
        assert!(is_retryable_error("Stream error: connection timed out"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_connection_reset() {
        assert!(is_retryable_error("Stream error: connection reset by peer"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_503() {
        assert!(is_retryable_error("HTTP 503 Service Unavailable"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_429() {
        assert!(is_retryable_error("HTTP 429 Too Many Requests"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_500() {
        assert!(is_retryable_error("HTTP 500 Internal Server Error"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_false_for_401() {
        assert!(!is_retryable_error("HTTP 401 Unauthorized"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_false_for_403() {
        assert!(!is_retryable_error("HTTP 403 Forbidden"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_false_for_400() {
        assert!(!is_retryable_error("HTTP 400 Bad Request"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_rate_limit() {
        assert!(is_retryable_error("rate limit exceeded, retry after 30s"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_dns() {
        assert!(is_retryable_error("dns resolution failed for host"));
    }

    // rtmx:req REQ-LLM-009
    #[test]
    fn is_retryable_error_returns_true_for_eof() {
        assert!(is_retryable_error("unexpected eof during stream read"));
    }

    // rtmx:req REQ-LLM-009
    #[tokio::test]
    async fn sse_parser_emits_retryable_error_on_stream_error() {
        // Simulate a stream that yields one chunk then an error.
        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> = vec![Ok(bytes::Bytes::from(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
        ))];
        // We cannot easily construct a reqwest::Error, so we use a stream
        // that yields data, then ends abruptly after partial output.
        // The SseTokenStream should detect the mid-stream drop.
        let s = stream::iter(chunks);
        let mut sse = SseTokenStream::new(s);

        // First event: token
        let event = sse.next().await.unwrap();
        match event {
            StreamEvent::Token(t) => assert_eq!(t, "Hi"),
            other => panic!("expected Token, got {:?}", other),
        }

        // Stream ends without [DONE] after partial output -> RetryableError
        let event = sse.next().await.unwrap();
        match event {
            StreamEvent::RetryableError { message, retryable } => {
                assert!(retryable, "mid-stream drop should be retryable");
                assert!(
                    message.contains("connection dropped") || message.contains("unexpectedly"),
                    "message should describe the drop: {message}"
                );
            }
            other => panic!("expected RetryableError, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-009
    #[tokio::test]
    async fn sse_parser_emits_done_for_empty_stream_without_output() {
        // An empty stream with no prior output should just emit Done, not error.
        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> = vec![];
        let s = stream::iter(chunks);
        let mut sse = SseTokenStream::new(s);

        let event = sse.next().await.unwrap();
        match event {
            StreamEvent::Done {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 0);
                assert_eq!(output_tokens, 0);
            }
            other => panic!("expected Done, got {:?}", other),
        }
    }
}
