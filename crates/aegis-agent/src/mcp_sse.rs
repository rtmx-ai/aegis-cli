//! SSE (Server-Sent Events) transport for MCP servers.
//!
//! Implements the HTTP+SSE transport per the MCP specification:
//! - HTTP POST for JSON-RPC requests
//! - SSE stream for server-to-client events
//! - Automatic reconnection with exponential backoff
//!
//! REQ-AGENT-056: SSE event parser
//! REQ-AGENT-057: SSE transport implementation
//! REQ-AGENT-058: SSE reconnection with backoff

use crate::mcp_types::{JsonRpcRequest, JsonRpcResponse};
use aegis_domain::error::DomainError;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// REQ-AGENT-056: SSE Event Parser
// ---------------------------------------------------------------------------

/// A parsed SSE event.
#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    /// Event type (from `event:` field). Defaults to "message".
    pub event_type: String,
    /// Event data (from `data:` field(s), joined with newlines).
    pub data: String,
    /// Event ID (from `id:` field).
    pub id: Option<String>,
    /// Retry interval in milliseconds (from `retry:` field).
    pub retry: Option<u64>,
}

impl SseEvent {
    /// Create a new event with default type "message".
    pub fn new(data: String) -> Self {
        Self {
            event_type: "message".to_string(),
            data,
            id: None,
            retry: None,
        }
    }
}

/// Parse a block of SSE text (one event, terminated by blank line) into
/// an `SseEvent`. Returns `None` if the block contains no data.
pub fn parse_sse_event(block: &str) -> Option<SseEvent> {
    let mut event_type = None;
    let mut data_lines: Vec<&str> = Vec::new();
    let mut id = None;
    let mut retry = None;

    for line in block.lines() {
        if line.is_empty() {
            continue; // skip blank lines within the block
        }
        if line.starts_with(':') {
            continue; // comment line
        }

        if let Some(value) = line.strip_prefix("event:") {
            event_type = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        } else if let Some(value) = line.strip_prefix("id:") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                id = Some(trimmed.to_string());
            }
        } else if let Some(value) = line.strip_prefix("retry:")
            && let Ok(ms) = value.trim().parse::<u64>()
        {
            retry = Some(ms);
        }
    }

    if data_lines.is_empty() {
        return None;
    }

    Some(SseEvent {
        event_type: event_type.unwrap_or_else(|| "message".to_string()),
        data: data_lines.join("\n"),
        id,
        retry,
    })
}

/// Parse a stream of SSE text (potentially multiple events separated by
/// blank lines) into a vector of events.
pub fn parse_sse_stream(text: &str) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let mut current_block = String::new();

    for line in text.lines() {
        if line.is_empty() {
            if !current_block.is_empty() {
                if let Some(event) = parse_sse_event(&current_block) {
                    events.push(event);
                }
                current_block.clear();
            }
        } else {
            if !current_block.is_empty() {
                current_block.push('\n');
            }
            current_block.push_str(line);
        }
    }

    // Handle trailing block without final blank line.
    if !current_block.is_empty()
        && let Some(event) = parse_sse_event(&current_block)
    {
        events.push(event);
    }

    events
}

// ---------------------------------------------------------------------------
// REQ-AGENT-058: SSE Reconnection with Backoff
// ---------------------------------------------------------------------------

/// Configuration for SSE reconnection behavior.
#[derive(Debug, Clone)]
pub struct SseReconnectConfig {
    /// Maximum number of reconnection attempts (default: 5).
    pub max_retries: u32,
    /// Initial backoff delay (default: 1s).
    pub initial_backoff: Duration,
    /// Maximum backoff delay (default: 30s).
    pub max_backoff: Duration,
    /// Backoff multiplier (default: 2.0).
    pub multiplier: f64,
}

impl Default for SseReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

impl SseReconnectConfig {
    /// Calculate the backoff delay for the given attempt number (0-based).
    pub fn backoff_delay(&self, attempt: u32) -> Duration {
        let delay_ms =
            self.initial_backoff.as_millis() as f64 * self.multiplier.powi(attempt as i32);
        let capped = delay_ms.min(self.max_backoff.as_millis() as f64);
        Duration::from_millis(capped as u64)
    }
}

// ---------------------------------------------------------------------------
// REQ-AGENT-057: SSE Transport
// ---------------------------------------------------------------------------

/// SSE transport state for an active MCP connection.
pub struct SseTransport {
    /// Base URL for the MCP server.
    base_url: String,
    /// Custom headers to include in requests.
    headers: HashMap<String, String>,
    /// HTTP client.
    client: reqwest::Client,
    /// Next JSON-RPC request ID.
    next_id: u64,
    /// Last received SSE event ID (for reconnection).
    last_event_id: Option<String>,
    /// Reconnection configuration.
    reconnect_config: SseReconnectConfig,
    /// The SSE endpoint URL (discovered during initialization).
    sse_endpoint: Option<String>,
    /// The message endpoint URL for sending requests.
    message_endpoint: Option<String>,
}

impl SseTransport {
    /// Create a new SSE transport.
    pub fn new(base_url: String, headers: HashMap<String, String>) -> Self {
        Self {
            base_url,
            headers,
            client: reqwest::Client::new(),
            next_id: 1,
            last_event_id: None,
            reconnect_config: SseReconnectConfig::default(),
            sse_endpoint: None,
            message_endpoint: None,
        }
    }

    /// Create with a custom HTTP client (for testing).
    pub fn with_client(
        base_url: String,
        headers: HashMap<String, String>,
        client: reqwest::Client,
    ) -> Self {
        Self {
            client,
            ..Self::new(base_url, headers)
        }
    }

    /// Set reconnection configuration.
    pub fn with_reconnect_config(mut self, config: SseReconnectConfig) -> Self {
        self.reconnect_config = config;
        self
    }

    /// Connect to the SSE endpoint and discover message endpoint.
    ///
    /// Per the MCP spec, the server exposes an SSE endpoint that sends
    /// an initial `endpoint` event containing the URL for sending messages.
    pub async fn connect(&mut self) -> Result<(), DomainError> {
        let sse_url = format!("{}/sse", self.base_url);
        self.sse_endpoint = Some(sse_url.clone());

        info!(url = %sse_url, "connecting to MCP SSE endpoint");

        let mut request = self.client.get(&sse_url);
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        if let Some(ref last_id) = self.last_event_id {
            request = request.header("Last-Event-ID", last_id.as_str());
        }

        let response = request
            .send()
            .await
            .map_err(|e| DomainError::Other(format!("SSE connection failed: {e}")))?;

        if !response.status().is_success() {
            return Err(DomainError::Other(format!(
                "SSE endpoint returned {}",
                response.status()
            )));
        }

        // Read the initial response to find the message endpoint.
        let body = response
            .text()
            .await
            .map_err(|e| DomainError::Other(format!("Failed to read SSE response: {e}")))?;

        let events = parse_sse_stream(&body);
        for event in &events {
            if event.event_type == "endpoint" {
                let endpoint = event.data.trim().to_string();
                // Resolve relative URLs against base.
                let resolved = if endpoint.starts_with("http") {
                    endpoint
                } else {
                    format!("{}{}", self.base_url, endpoint)
                };
                self.message_endpoint = Some(resolved.clone());
                info!(endpoint = %resolved, "discovered MCP message endpoint");
            }
            if let Some(ref id) = event.id {
                self.last_event_id = Some(id.clone());
            }
        }

        if self.message_endpoint.is_none() {
            return Err(DomainError::Other(
                "SSE server did not send endpoint event".to_string(),
            ));
        }

        Ok(())
    }

    /// Send a JSON-RPC request and receive a response via the message endpoint.
    pub async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, DomainError> {
        let endpoint = self.message_endpoint.as_ref().ok_or_else(|| {
            DomainError::Other("Not connected: no message endpoint".to_string())
        })?;

        let id = self.next_id;
        self.next_id += 1;

        let request = if let Some(p) = params {
            JsonRpcRequest::with_params(id, method, p)
        } else {
            JsonRpcRequest::new(id, method)
        };

        debug!(method, id, "sending SSE JSON-RPC request");

        let mut req = self.client.post(endpoint);
        req = req.header("Content-Type", "application/json");
        for (key, value) in &self.headers {
            req = req.header(key.as_str(), value.as_str());
        }
        req = req.json(&request);

        let response = req
            .send()
            .await
            .map_err(|e| DomainError::Other(format!("SSE request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Other(format!(
                "SSE server returned {status}: {body}"
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| DomainError::Other(format!("Failed to read SSE response body: {e}")))?;

        // The response may come as SSE events or direct JSON.
        // Try SSE parse first, fall back to direct JSON.
        let events = parse_sse_stream(&body);
        for event in &events {
            if event.event_type == "message" {
                let parsed: JsonRpcResponse = serde_json::from_str(&event.data).map_err(|e| {
                    DomainError::Other(format!("Failed to parse JSON-RPC response from SSE: {e}"))
                })?;
                if let Some(ref id) = event.id {
                    self.last_event_id = Some(id.clone());
                }
                return Ok(parsed);
            }
        }

        // Fall back to direct JSON parsing.
        serde_json::from_str(&body)
            .map_err(|e| DomainError::Other(format!("Failed to parse JSON-RPC response: {e}")))
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn send_notification(&mut self, method: &str) -> Result<(), DomainError> {
        let endpoint = self.message_endpoint.as_ref().ok_or_else(|| {
            DomainError::Other("Not connected: no message endpoint".to_string())
        })?;

        let notif = JsonRpcRequest::notification(method);

        let mut req = self.client.post(endpoint);
        req = req.header("Content-Type", "application/json");
        for (key, value) in &self.headers {
            req = req.header(key.as_str(), value.as_str());
        }
        req = req.json(&notif);

        let response = req
            .send()
            .await
            .map_err(|e| DomainError::Other(format!("SSE notification failed: {e}")))?;

        if !response.status().is_success() {
            warn!(
                status = %response.status(),
                "SSE notification returned non-success"
            );
        }

        Ok(())
    }

    /// Attempt to reconnect with exponential backoff (REQ-AGENT-058).
    ///
    /// Retries up to `max_retries` times with increasing delays.
    /// Sends `Last-Event-ID` header to resume from the last received event.
    pub async fn reconnect(&mut self) -> Result<(), DomainError> {
        for attempt in 0..self.reconnect_config.max_retries {
            let delay = self.reconnect_config.backoff_delay(attempt);
            warn!(
                attempt = attempt + 1,
                max = self.reconnect_config.max_retries,
                delay_ms = delay.as_millis() as u64,
                last_event_id = ?self.last_event_id,
                "SSE reconnection attempt"
            );

            tokio::time::sleep(delay).await;

            match self.connect().await {
                Ok(()) => {
                    info!(attempt = attempt + 1, "SSE reconnection successful");
                    return Ok(());
                }
                Err(e) => {
                    warn!(attempt = attempt + 1, %e, "SSE reconnection failed");
                }
            }
        }

        Err(DomainError::Other(format!(
            "SSE reconnection failed after {} attempts",
            self.reconnect_config.max_retries
        )))
    }

    /// Get the last received event ID.
    pub fn last_event_id(&self) -> Option<&str> {
        self.last_event_id.as_deref()
    }

    /// Get the next request ID (for testing).
    pub fn next_id(&self) -> u64 {
        self.next_id
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- REQ-AGENT-056: SSE Event Parser ---

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_data_field() {
        let block = "data: hello world\n";
        let event = parse_sse_event(block).unwrap();
        assert_eq!(event.data, "hello world");
        assert_eq!(event.event_type, "message");
    }

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_multiline_data() {
        let block = "data: line one\ndata: line two\ndata: line three\n";
        let event = parse_sse_event(block).unwrap();
        assert_eq!(event.data, "line one\nline two\nline three");
    }

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_event_type() {
        let block = "event: endpoint\ndata: /messages\n";
        let event = parse_sse_event(block).unwrap();
        assert_eq!(event.event_type, "endpoint");
        assert_eq!(event.data, "/messages");
    }

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_id_and_retry() {
        let block = "id: 42\nretry: 5000\ndata: payload\n";
        let event = parse_sse_event(block).unwrap();
        assert_eq!(event.id, Some("42".to_string()));
        assert_eq!(event.retry, Some(5000));
        assert_eq!(event.data, "payload");
    }

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_comment_lines_ignored() {
        let block = ": this is a comment\ndata: actual data\n";
        let event = parse_sse_event(block).unwrap();
        assert_eq!(event.data, "actual data");
    }

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_empty_block_returns_none() {
        assert!(parse_sse_event("").is_none());
        assert!(parse_sse_event(": comment only\n").is_none());
    }

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_stream_multiple_events() {
        let text = "data: first\n\ndata: second\n\nevent: custom\ndata: third\n\n";
        let events = parse_sse_stream(text);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
        assert_eq!(events[2].event_type, "custom");
        assert_eq!(events[2].data, "third");
    }

    // rtmx:req REQ-AGENT-056
    #[test]
    fn test_parse_sse_json_rpc_data() {
        let block = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n";
        let event = parse_sse_event(block).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&event.data).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
    }

    // --- REQ-AGENT-057: SSE Transport ---

    // rtmx:req REQ-AGENT-057
    #[tokio::test]
    async fn test_sse_connect_and_initialize() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock the SSE endpoint that returns the message endpoint.
        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("event: endpoint\ndata: /messages\n\n"),
            )
            .mount(&mock_server)
            .await;

        let mut transport = SseTransport::new(mock_server.uri(), HashMap::new());
        transport.connect().await.unwrap();

        assert!(transport.message_endpoint.is_some());
        assert!(
            transport
                .message_endpoint
                .as_ref()
                .unwrap()
                .ends_with("/messages")
        );
    }

    // rtmx:req REQ-AGENT-057
    #[tokio::test]
    async fn test_sse_send_request_roundtrip() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // SSE endpoint.
        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("event: endpoint\ndata: /messages\n\n"),
            )
            .mount(&mock_server)
            .await;

        // Message endpoint returns JSON-RPC response.
        let json_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "serverInfo": {"name": "test", "version": "1.0"}
            }
        });
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&json_response))
            .mount(&mock_server)
            .await;

        let mut transport = SseTransport::new(mock_server.uri(), HashMap::new());
        transport.connect().await.unwrap();

        let response = transport
            .send_request(
                "initialize",
                Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "aegis", "version": "0.1.0"}
                })),
            )
            .await
            .unwrap();

        assert!(response.result.is_some());
    }

    // rtmx:req REQ-AGENT-057
    #[tokio::test]
    async fn test_sse_connect_no_endpoint_event_fails() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // SSE endpoint returns no endpoint event.
        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: hello\n\n"))
            .mount(&mock_server)
            .await;

        let mut transport = SseTransport::new(mock_server.uri(), HashMap::new());
        let result = transport.connect().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("endpoint"));
    }

    // rtmx:req REQ-AGENT-057
    #[tokio::test]
    async fn test_sse_connect_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let mut transport = SseTransport::new(mock_server.uri(), HashMap::new());
        let result = transport.connect().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("500"));
    }

    // rtmx:req REQ-AGENT-057
    #[tokio::test]
    async fn test_sse_send_without_connect_fails() {
        let mut transport = SseTransport::new("http://localhost:1".to_string(), HashMap::new());
        let result = transport.send_request("test", None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not connected"));
    }

    // --- REQ-AGENT-058: SSE Reconnection ---

    // rtmx:req REQ-AGENT-058
    #[test]
    fn test_sse_backoff_delay_increases() {
        let config = SseReconnectConfig {
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            multiplier: 2.0,
            max_retries: 5,
        };

        let d0 = config.backoff_delay(0);
        let d1 = config.backoff_delay(1);
        let d2 = config.backoff_delay(2);

        assert_eq!(d0, Duration::from_millis(100));
        assert_eq!(d1, Duration::from_millis(200));
        assert_eq!(d2, Duration::from_millis(400));
    }

    // rtmx:req REQ-AGENT-058
    #[test]
    fn test_sse_backoff_caps_at_max() {
        let config = SseReconnectConfig {
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(5),
            multiplier: 10.0,
            max_retries: 5,
        };

        // 1 * 10^2 = 100s, but capped at 5s.
        let d2 = config.backoff_delay(2);
        assert_eq!(d2, Duration::from_secs(5));
    }

    // rtmx:req REQ-AGENT-058
    #[tokio::test]
    async fn test_sse_reconnect_with_backoff() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // First two attempts fail, third succeeds.
        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("event: endpoint\ndata: /messages\n\n"),
            )
            .mount(&mock_server)
            .await;

        let mut transport = SseTransport::new(mock_server.uri(), HashMap::new())
            .with_reconnect_config(SseReconnectConfig {
                max_retries: 5,
                initial_backoff: Duration::from_millis(10),
                max_backoff: Duration::from_millis(100),
                multiplier: 2.0,
            });

        // First connect fails.
        let _ = transport.connect().await;

        // Reconnect should succeed after retries.
        transport.reconnect().await.unwrap();
        assert!(transport.message_endpoint.is_some());
    }

    // rtmx:req REQ-AGENT-058
    #[tokio::test]
    async fn test_sse_reconnect_exhausts_retries() {
        let mut transport = SseTransport::new("http://127.0.0.1:1".to_string(), HashMap::new())
            .with_reconnect_config(SseReconnectConfig {
                max_retries: 2,
                initial_backoff: Duration::from_millis(10),
                max_backoff: Duration::from_millis(50),
                multiplier: 2.0,
            });

        let result = transport.reconnect().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("2 attempts"));
    }

    // rtmx:req REQ-AGENT-058
    #[test]
    fn test_sse_reconnect_default_config() {
        let config = SseReconnectConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.max_backoff, Duration::from_secs(30));
    }

    // rtmx:req REQ-AGENT-058
    #[tokio::test]
    async fn test_sse_reconnect_preserves_last_event_id() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Initial connect succeeds and sets an event ID.
        Mock::given(method("GET"))
            .and(path("/sse"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("id: evt-42\nevent: endpoint\ndata: /messages\n\n"),
            )
            .mount(&mock_server)
            .await;

        let mut transport = SseTransport::new(mock_server.uri(), HashMap::new())
            .with_reconnect_config(SseReconnectConfig {
                max_retries: 3,
                initial_backoff: Duration::from_millis(10),
                max_backoff: Duration::from_millis(50),
                multiplier: 2.0,
            });

        transport.connect().await.unwrap();
        assert_eq!(transport.last_event_id(), Some("evt-42"));

        // On reconnect, verify Last-Event-ID header is sent.
        // We set up a mock that requires the header.
        mock_server.reset().await;
        Mock::given(method("GET"))
            .and(path("/sse"))
            .and(header("Last-Event-ID", "evt-42"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("event: endpoint\ndata: /messages\n\n"),
            )
            .mount(&mock_server)
            .await;

        transport.reconnect().await.unwrap();
    }
}
