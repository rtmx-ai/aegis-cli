//! Bedrock event-stream parser for the Converse Stream API.
//!
//! AWS Bedrock returns responses in the `application/vnd.amazon.eventstream`
//! binary framing format. Each frame contains a JSON payload with an
//! `:event-type` header indicating the event kind. This module parses those
//! frames into `StreamEvent` values.
//!
//! Binary frame format (AWS event-stream):
//! - 4 bytes: total length (big-endian)
//! - 4 bytes: headers length (big-endian)
//! - 4 bytes: prelude CRC
//! - headers (variable length)
//! - payload (variable length)
//! - 4 bytes: message CRC

use aegis_domain::ports::*;
use async_trait::async_trait;

use crate::sse::parse_tool_call;

/// Token stream that parses Bedrock event-stream binary frames.
pub struct BedrockTokenStream {
    buffer: Vec<u8>,
    done: bool,
    input_tokens: u64,
    output_tokens: u64,
    /// Accumulates tool use ID and input across contentBlockStart/Delta/Stop.
    pending_tool_id: Option<String>,
    pending_tool_name: Option<String>,
    pending_tool_input: String,
    inner: Box<dyn futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin>,
}

impl BedrockTokenStream {
    pub fn new<S>(stream: S) -> Self
    where
        S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
    {
        Self {
            buffer: Vec::new(),
            done: false,
            input_tokens: 0,
            output_tokens: 0,
            pending_tool_id: None,
            pending_tool_name: None,
            pending_tool_input: String::new(),
            inner: Box::new(stream),
        }
    }

    /// Try to decode the next complete event-stream frame from the buffer.
    ///
    /// Returns `Some((event_type, payload_json))` if a complete frame is
    /// available, `None` if more data is needed.
    fn try_decode_frame(&mut self) -> Option<(String, serde_json::Value)> {
        // Minimum frame size: 4 (total_len) + 4 (headers_len) + 4
        // (prelude_crc) + 0 (headers) + 0 (payload) + 4 (message_crc) = 16
        if self.buffer.len() < 12 {
            return None;
        }

        let total_length = u32::from_be_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]) as usize;

        if self.buffer.len() < total_length {
            return None; // Need more data
        }

        let headers_length = u32::from_be_bytes([
            self.buffer[4],
            self.buffer[5],
            self.buffer[6],
            self.buffer[7],
        ]) as usize;

        // Prelude is 12 bytes (total_len + headers_len + prelude_crc)
        let headers_start = 12;
        let headers_end = headers_start + headers_length;
        let payload_start = headers_end;
        // Payload ends 4 bytes before total_length (message CRC)
        let payload_end = total_length - 4;

        // Parse headers to find :event-type
        let event_type = parse_event_type(&self.buffer[headers_start..headers_end]);

        // Parse payload as JSON
        let payload_bytes = &self.buffer[payload_start..payload_end];
        let payload: serde_json::Value = if payload_bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(payload_bytes).unwrap_or_default()
        };

        // Consume the frame from the buffer
        self.buffer.drain(..total_length);

        Some((event_type, payload))
    }

    /// Process a decoded event-stream frame into a StreamEvent.
    fn process_event(
        &mut self,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> Option<StreamEvent> {
        match event_type {
            "contentBlockDelta" => {
                // Text delta: {"delta": {"text": "..."}}
                if let Some(text) = payload["delta"]["text"].as_str() {
                    return Some(StreamEvent::Token(text.to_string()));
                }
                // Tool input delta: {"delta": {"toolUse": {"input": "..."}}}
                if let Some(input) = payload["delta"]["toolUse"]["input"].as_str() {
                    self.pending_tool_input.push_str(input);
                }
                None
            }
            "contentBlockStart" => {
                // Tool use start: {"start": {"toolUse": {"toolUseId": "...",
                // "name": "..."}}}
                if let Some(tool_use) = payload["start"]["toolUse"].as_object() {
                    self.pending_tool_id = tool_use
                        .get("toolUseId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    self.pending_tool_name = tool_use
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    self.pending_tool_input.clear();
                }
                None
            }
            "contentBlockStop" => {
                // If we were accumulating a tool call, emit it now
                if let Some(name) = self.pending_tool_name.take() {
                    self.pending_tool_id.take();
                    let input = std::mem::take(&mut self.pending_tool_input);
                    return parse_tool_call(&name, &input);
                }
                None
            }
            "messageStop" => {
                self.done = true;
                Some(StreamEvent::Done {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                })
            }
            "metadata" => {
                // {"usage": {"inputTokens": N, "outputTokens": N}}
                if let Some(usage) = payload.get("usage") {
                    if let Some(input) = usage["inputTokens"].as_u64() {
                        self.input_tokens = input;
                    }
                    if let Some(output) = usage["outputTokens"].as_u64() {
                        self.output_tokens = output;
                    }
                }
                None
            }
            "exception" | "error" => {
                let message = payload["message"]
                    .as_str()
                    .unwrap_or("Unknown Bedrock error")
                    .to_string();
                self.done = true;
                Some(StreamEvent::Error(message))
            }
            _ => {
                tracing::trace!(
                    event_type = event_type,
                    "ignoring unknown Bedrock event type"
                );
                None
            }
        }
    }
}

/// Parse event-stream headers to extract the `:event-type` value.
///
/// Header format (per AWS spec):
/// - 1 byte: header name length
/// - N bytes: header name (UTF-8)
/// - 1 byte: header value type (7 = string)
/// - 2 bytes: string value length (big-endian)
/// - N bytes: string value (UTF-8)
fn parse_event_type(headers: &[u8]) -> String {
    let mut offset = 0;
    while offset < headers.len() {
        // Read header name length
        let name_len = headers[offset] as usize;
        offset += 1;
        if offset + name_len > headers.len() {
            break;
        }
        let name = std::str::from_utf8(&headers[offset..offset + name_len]).unwrap_or("");
        offset += name_len;

        // Read value type
        if offset >= headers.len() {
            break;
        }
        let value_type = headers[offset];
        offset += 1;

        if value_type == 7 {
            // String type
            if offset + 2 > headers.len() {
                break;
            }
            let value_len = u16::from_be_bytes([headers[offset], headers[offset + 1]]) as usize;
            offset += 2;
            if offset + value_len > headers.len() {
                break;
            }
            let value = std::str::from_utf8(&headers[offset..offset + value_len]).unwrap_or("");
            offset += value_len;

            if name == ":event-type" {
                return value.to_string();
            }
        } else {
            // Skip unknown value types -- we only care about strings
            // Other types have different length encodings; bail out
            break;
        }
    }
    String::new()
}

/// Build a raw event-stream frame for testing.
///
/// Constructs a binary frame with the given event type and JSON payload,
/// using proper AWS event-stream binary encoding.
#[cfg(test)]
fn build_test_frame(event_type: &str, payload: &serde_json::Value) -> Vec<u8> {
    let payload_bytes = serde_json::to_vec(payload).unwrap();

    // Build headers: ":event-type" = event_type (string, type 7)
    let mut headers = Vec::new();
    let name = b":event-type";
    headers.push(name.len() as u8);
    headers.extend_from_slice(name);
    headers.push(7); // string type
    let et_bytes = event_type.as_bytes();
    headers.extend_from_slice(&(et_bytes.len() as u16).to_be_bytes());
    headers.extend_from_slice(et_bytes);

    // Also add :message-type = "event" header
    let mt_name = b":message-type";
    headers.push(mt_name.len() as u8);
    headers.extend_from_slice(mt_name);
    headers.push(7);
    let mt_val = b"event";
    headers.extend_from_slice(&(mt_val.len() as u16).to_be_bytes());
    headers.extend_from_slice(mt_val);

    let headers_length = headers.len() as u32;
    // Total = 12 (prelude) + headers + payload + 4 (message CRC)
    let total_length = 12 + headers.len() + payload_bytes.len() + 4;

    let mut frame = Vec::with_capacity(total_length);
    frame.extend_from_slice(&(total_length as u32).to_be_bytes());
    frame.extend_from_slice(&headers_length.to_be_bytes());
    // Prelude CRC (placeholder -- real CRC not validated in our parser)
    frame.extend_from_slice(&[0u8; 4]);
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(&payload_bytes);
    // Message CRC (placeholder)
    frame.extend_from_slice(&[0u8; 4]);

    frame
}

#[async_trait]
impl TokenStream for BedrockTokenStream {
    async fn next(&mut self) -> Option<StreamEvent> {
        use futures::StreamExt;

        loop {
            if self.done {
                return None;
            }

            // Try to decode frames from the buffer
            while let Some((event_type, payload)) = self.try_decode_frame() {
                if let Some(event) = self.process_event(&event_type, &payload) {
                    return Some(event);
                }
            }

            // Read more data from the underlying stream
            match self.inner.next().await {
                Some(Ok(bytes)) => {
                    self.buffer.extend_from_slice(&bytes);
                }
                Some(Err(e)) => {
                    self.done = true;
                    return Some(StreamEvent::RetryableError {
                        message: format!("Stream error: {e}"),
                        retryable: true,
                    });
                }
                None => {
                    if !self.done {
                        self.done = true;
                        if self.output_tokens > 0 {
                            return Some(StreamEvent::RetryableError {
                                message: "Bedrock stream ended \
                                         unexpectedly (connection \
                                         dropped)"
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    // rtmx:req REQ-LLM-002
    #[tokio::test]
    async fn stream_parses_content_delta() {
        let frame = build_test_frame(
            "contentBlockDelta",
            &serde_json::json!({
                "delta": {"text": "Hello world"}
            }),
        );

        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> =
            vec![Ok(bytes::Bytes::from(frame))];
        let s = stream::iter(chunks);
        let mut stream = BedrockTokenStream::new(s);

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Token(t) => assert_eq!(t, "Hello world"),
            other => panic!("expected Token, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-002
    #[tokio::test]
    async fn stream_parses_tool_use() {
        // contentBlockStart with toolUse
        let start_frame = build_test_frame(
            "contentBlockStart",
            &serde_json::json!({
                "start": {
                    "toolUse": {
                        "toolUseId": "tool-123",
                        "name": "read_file"
                    }
                }
            }),
        );

        // contentBlockDelta with tool input
        let delta_frame = build_test_frame(
            "contentBlockDelta",
            &serde_json::json!({
                "delta": {
                    "toolUse": {
                        "input": "{\"path\":\"src/main.rs\"}"
                    }
                }
            }),
        );

        // contentBlockStop to finalize
        let stop_frame = build_test_frame("contentBlockStop", &serde_json::json!({}));

        let mut all_bytes = Vec::new();
        all_bytes.extend_from_slice(&start_frame);
        all_bytes.extend_from_slice(&delta_frame);
        all_bytes.extend_from_slice(&stop_frame);

        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> =
            vec![Ok(bytes::Bytes::from(all_bytes))];
        let s = stream::iter(chunks);
        let mut stream = BedrockTokenStream::new(s);

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::ToolUse(tool_call) => {
                // The parse_tool_call function maps "read_file" to
                // ToolCall::ReadFile
                match tool_call {
                    aegis_domain::types::ToolCall::ReadFile { path } => {
                        assert_eq!(path.as_path().to_str().unwrap(), "src/main.rs");
                    }
                    other => panic!("expected ReadFile, got {:?}", other),
                }
            }
            other => {
                panic!("expected ToolUse, got {:?}", other)
            }
        }
    }

    // rtmx:req REQ-LLM-002
    #[tokio::test]
    async fn stream_tracks_usage() {
        let metadata_frame = build_test_frame(
            "metadata",
            &serde_json::json!({
                "usage": {
                    "inputTokens": 42,
                    "outputTokens": 17
                }
            }),
        );

        let stop_frame = build_test_frame(
            "messageStop",
            &serde_json::json!({"stopReason": "end_turn"}),
        );

        let mut all_bytes = Vec::new();
        all_bytes.extend_from_slice(&metadata_frame);
        all_bytes.extend_from_slice(&stop_frame);

        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> =
            vec![Ok(bytes::Bytes::from(all_bytes))];
        let s = stream::iter(chunks);
        let mut stream = BedrockTokenStream::new(s);

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Done {
                input_tokens,
                output_tokens,
            } => {
                assert_eq!(input_tokens, 42);
                assert_eq!(output_tokens, 17);
            }
            other => panic!("expected Done, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-002
    #[tokio::test]
    async fn stream_handles_error() {
        let error_frame = build_test_frame(
            "exception",
            &serde_json::json!({
                "message": "Model not found"
            }),
        );

        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> =
            vec![Ok(bytes::Bytes::from(error_frame))];
        let s = stream::iter(chunks);
        let mut stream = BedrockTokenStream::new(s);

        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Error(msg) => {
                assert_eq!(msg, "Model not found");
            }
            other => panic!("expected Error, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn parse_event_type_extracts_correct_value() {
        // Build a header block with :event-type = "contentBlockDelta"
        let mut headers = Vec::new();
        let name = b":event-type";
        headers.push(name.len() as u8);
        headers.extend_from_slice(name);
        headers.push(7); // string type
        let val = b"contentBlockDelta";
        headers.extend_from_slice(&(val.len() as u16).to_be_bytes());
        headers.extend_from_slice(val);

        let result = parse_event_type(&headers);
        assert_eq!(result, "contentBlockDelta");
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn parse_event_type_returns_empty_for_missing_header() {
        let headers: Vec<u8> = Vec::new();
        let result = parse_event_type(&headers);
        assert_eq!(result, "");
    }
}
