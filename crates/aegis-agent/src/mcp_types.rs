//! JSON-RPC 2.0 types for MCP (Model Context Protocol) communication.
//!
//! These types implement the wire format for JSON-RPC 2.0 messages used
//! by the MCP protocol over stdio (NDJSON) and HTTP+SSE transports.

use serde::{Deserialize, Serialize};

/// A JSON-RPC 2.0 request or notification.
///
/// When `id` is `None`, the message is a notification (no response expected).
/// When `id` is `Some(n)`, a response with the same `id` is expected.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    /// Always "2.0" per the JSON-RPC specification.
    pub jsonrpc: &'static str,
    /// Request ID. `None` for notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// The method to invoke on the server.
    pub method: String,
    /// Parameters for the method (may be empty object `{}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Create a new request with an ID (expects a response).
    pub fn new(id: u64, method: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: Some(id),
            method: method.into(),
            params: Some(serde_json::json!({})),
        }
    }

    /// Create a new request with parameters.
    pub fn with_params(id: u64, method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id: Some(id),
            method: method.into(),
            params: Some(params),
        }
    }

    /// Create a notification (no ID, no response expected).
    pub fn notification(method: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: None,
            method: method.into(),
            params: None,
        }
    }
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    /// Always "2.0".
    pub jsonrpc: String,
    /// The ID matching the request. `None` for notifications.
    pub id: Option<u64>,
    /// The result on success.
    pub result: Option<serde_json::Value>,
    /// The error on failure.
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Returns `true` if this response indicates success (has result, no error).
    pub fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional error data.
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-AGENT-022
    #[test]
    fn jsonrpc_request_serializes_correctly() {
        let req = JsonRpcRequest::new(1, "initialize");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "initialize");
        assert_eq!(json["params"], serde_json::json!({}));
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn jsonrpc_response_parses_result() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"protocolVersion": "2024-11-05"}
        }"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(1));
        assert!(resp.is_success());
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn jsonrpc_response_parses_error() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "error": {"code": -32601, "message": "Method not found"}
        }"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(2));
        assert!(!resp.is_success());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn notification_has_no_id() {
        let notif = JsonRpcRequest::notification("notifications/initialized");
        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert!(json.get("id").is_none());
        assert_eq!(json["method"], "notifications/initialized");
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn initialize_handshake_format() {
        let req = JsonRpcRequest::with_params(
            1,
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "aegis",
                    "version": "0.1.0"
                }
            }),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["method"], "initialize");
        let params = &json["params"];
        assert_eq!(params["protocolVersion"], "2024-11-05");
        assert_eq!(params["clientInfo"]["name"], "aegis");
        assert_eq!(params["clientInfo"]["version"], "0.1.0");
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn tools_list_request_format() {
        let req = JsonRpcRequest::new(2, "tools/list");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["method"], "tools/list");
        assert_eq!(json["id"], 2);
        assert_eq!(json["jsonrpc"], "2.0");
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn tools_call_request_format() {
        let req = JsonRpcRequest::with_params(
            3,
            "tools/call",
            serde_json::json!({
                "name": "query_db",
                "arguments": {"sql": "SELECT 1"}
            }),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["method"], "tools/call");
        assert_eq!(json["params"]["name"], "query_db");
        assert_eq!(json["params"]["arguments"]["sql"], "SELECT 1");
    }
}
