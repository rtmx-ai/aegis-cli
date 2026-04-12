//! Wiremock-backed OpenAI-compatible LLM provider for deterministic E2E tests.
//!
//! Spawns a [`wiremock::MockServer`] preconfigured to answer
//! `POST /v1/chat/completions` with canned Server-Sent Event streams in the
//! exact format expected by [`aegis_llm::local::LocalProvider`]. Tests can
//! point a real `LocalProvider` (or any HTTP client) at [`WireMockLlm::endpoint`]
//! and exercise the full chat path without a real model.
//!
//! # Example
//!
//! ```ignore
//! use aegis_test_support::wiremock_llm::WireMockLlm;
//!
//! # async fn run() {
//! let llm = WireMockLlm::new()
//!     .await
//!     .with_streaming_response("Hello world")
//!     .await
//!     .with_done(10, 2)
//!     .await;
//!
//! let url = format!("{}/chat/completions", llm.endpoint());
//! // ... POST to url, parse SSE ...
//! # }
//! ```

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Wiremock-backed OpenAI-compatible LLM endpoint.
///
/// Each `with_*` builder registers an additional mock on the underlying
/// [`MockServer`]. Mocks are matched in registration order, so chained calls
/// produce a deterministic response sequence.
pub struct WireMockLlm {
    server: MockServer,
    body: String,
}

impl WireMockLlm {
    /// Spawn a fresh mock server on a random localhost port.
    pub async fn new() -> Self {
        Self {
            server: MockServer::start().await,
            body: String::new(),
        }
    }

    /// Base URL ending in `/v1` (matches the shape `LocalProvider` expects).
    pub fn endpoint(&self) -> String {
        format!("{}/v1", self.server.uri())
    }

    /// Append SSE chunks tokenizing `text` (one chunk per whitespace-delimited
    /// word, preserving the leading space) to the canned response. The mock is
    /// (re)mounted so that the next request returns the accumulated body.
    pub async fn with_streaming_response(mut self, text: &str) -> Self {
        // Tokenize on spaces, preserving leading spaces on all words after
        // the first so reassembly via concatenation reproduces `text` exactly.
        let mut first = true;
        for word in text.split(' ') {
            let chunk_text = if first {
                first = false;
                word.to_string()
            } else {
                format!(" {word}")
            };
            let escaped = serde_json::to_string(&chunk_text).expect("string serializes");
            self.body.push_str(&format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":{escaped}}}}}]}}\n\n"
            ));
        }
        self.remount().await;
        self
    }

    /// Register a streaming tool_call response in OpenAI format.
    pub async fn with_tool_call(mut self, tool_name: &str, args: serde_json::Value) -> Self {
        let args_str = args.to_string();
        let chunk = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "function": {
                            "name": tool_name,
                            "arguments": args_str,
                        }
                    }]
                }
            }]
        });
        self.body
            .push_str(&format!("data: {chunk}\n\n", chunk = chunk));
        self.remount().await;
        self
    }

    /// Append a usage frame followed by the terminal `[DONE]` marker.
    pub async fn with_done(mut self, input_tokens: u64, output_tokens: u64) -> Self {
        let usage = json!({
            "choices": [{"delta": {}}],
            "usage": {
                "prompt_tokens": input_tokens,
                "completion_tokens": output_tokens,
            }
        });
        self.body.push_str(&format!("data: {usage}\n\n"));
        self.body.push_str("data: [DONE]\n\n");
        self.remount().await;
        self
    }

    async fn remount(&self) {
        // wiremock has no "replace" API, so we reset and remount the cumulative
        // body. This is fine: builder methods are called sequentially before
        // the test issues any HTTP requests.
        self.server.reset().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(self.body.clone(), "text/event-stream"),
            )
            .mount(&self.server)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    // rtmx:req REQ-TEST-022
    #[tokio::test]
    async fn test_wiremock_llm_provides_endpoint() {
        let llm = WireMockLlm::new().await;
        let endpoint = llm.endpoint();
        let url = reqwest::Url::parse(&endpoint).expect("endpoint parses as URL");
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.path(), "/v1");
        assert!(url.host_str().is_some());
        assert!(url.port().is_some());
    }

    // rtmx:req REQ-TEST-022
    #[tokio::test]
    async fn test_wiremock_llm_streams_text() {
        let llm = WireMockLlm::new()
            .await
            .with_streaming_response("Hello world from aegis")
            .await
            .with_done(7, 4)
            .await;

        let url = format!("{}/chat/completions", llm.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&serde_json::json!({"model": "x", "messages": []}))
            .send()
            .await
            .expect("request succeeds");
        assert!(resp.status().is_success());

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.expect("stream chunk");
            buf.push_str(std::str::from_utf8(&bytes).expect("utf8"));
        }

        // Reassemble content tokens.
        let mut reassembled = String::new();
        let mut saw_done = false;
        for line in buf.lines() {
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                saw_done = true;
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(data).expect("valid json");
            if let Some(content) = v["choices"][0]["delta"]["content"].as_str() {
                reassembled.push_str(content);
            }
        }
        assert_eq!(reassembled, "Hello world from aegis");
        assert!(saw_done, "stream must end with [DONE]");
    }

    // rtmx:req REQ-TEST-022
    #[tokio::test]
    async fn test_wiremock_llm_responds_with_tool_call() {
        let llm = WireMockLlm::new()
            .await
            .with_tool_call("read_file", serde_json::json!({"path": "src/main.rs"}))
            .await
            .with_done(5, 1)
            .await;

        let url = format!("{}/chat/completions", llm.endpoint());
        let body = reqwest::Client::new()
            .post(&url)
            .json(&serde_json::json!({"model": "x", "messages": []}))
            .send()
            .await
            .expect("request succeeds")
            .text()
            .await
            .expect("body");

        let mut found_tool_call = false;
        for line in body.lines() {
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(data).expect("valid json");
            if let Some(tc) = v["choices"][0]["delta"]["tool_calls"][0]["function"].as_object() {
                assert_eq!(tc["name"].as_str(), Some("read_file"));
                let args_str = tc["arguments"].as_str().expect("arguments string");
                let args: serde_json::Value = serde_json::from_str(args_str).expect("args json");
                assert_eq!(args["path"], "src/main.rs");
                found_tool_call = true;
            }
        }
        assert!(found_tool_call, "expected tool_call in SSE stream");
    }
}
