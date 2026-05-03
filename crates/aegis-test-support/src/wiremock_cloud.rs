//! Wiremock-backed cloud-provider LLM stubs for deterministic E2E tests.
//!
//! Provides [`WireMockVertex`], [`WireMockBedrock`], and [`WireMockAzure`] --
//! provider-specific HTTP stubs that validate auth headers and return canned
//! responses in each provider's native format.
//!
//! # Provider formats
//!
//! - **Vertex AI (Gemini):** SSE with `{"candidates":[{"content":{"parts":[{"text":"..."}]}}]}`
//! - **Bedrock (Converse):** JSON with `{"output":{"message":{"content":[{"text":"..."}]}}}`
//! - **Azure OpenAI:** SSE in OpenAI format with Azure-specific wrapper fields

use serde_json::json;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

// ---------------------------------------------------------------------------
// Custom matchers
// ---------------------------------------------------------------------------

/// Matches requests that have an `Authorization` header starting with `Bearer `.
struct BearerTokenMatcher;

impl Match for BearerTokenMatcher {
    fn matches(&self, request: &Request) -> bool {
        request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("Bearer "))
    }
}

/// Matches requests that have an `Authorization` header containing `AWS4-HMAC-SHA256`.
struct AwsSigV4Matcher;

impl Match for AwsSigV4Matcher {
    fn matches(&self, request: &Request) -> bool {
        request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("AWS4-HMAC-SHA256"))
    }
}

/// Matches requests that have either `api-key` header or `Authorization: Bearer`.
struct AzureAuthMatcher;

impl Match for AzureAuthMatcher {
    fn matches(&self, request: &Request) -> bool {
        let has_api_key = request.headers.get("api-key").is_some();
        let has_bearer = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("Bearer "));
        has_api_key || has_bearer
    }
}

// ===========================================================================
// WireMockVertex
// ===========================================================================

/// Wiremock stub for Google Vertex AI (Gemini) endpoints.
///
/// Validates `Authorization: Bearer <token>` and returns Gemini-style SSE.
pub struct WireMockVertex {
    server: MockServer,
    body: String,
}

impl WireMockVertex {
    /// Spawn a fresh mock server with a default empty response.
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let stub = Self {
            server,
            body: String::new(),
        };
        stub.mount_auth_reject().await;
        stub
    }

    /// Base URL for the mock server.
    pub fn endpoint(&self) -> String {
        self.server.uri()
    }

    /// Configure the stub to return a canned text response as Gemini SSE.
    pub async fn with_response(mut self, text: &str) -> Self {
        let chunk = json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": text}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        });
        self.body
            .push_str(&format!("data: {chunk}\n\ndata: [DONE]\n\n"));
        self.remount().await;
        self
    }

    /// Configure the stub to return a tool call in Gemini format.
    pub async fn with_tool_call(mut self, name: &str, args: serde_json::Value) -> Self {
        let chunk = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": name,
                            "args": args
                        }
                    }],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        });
        self.body
            .push_str(&format!("data: {chunk}\n\ndata: [DONE]\n\n"));
        self.remount().await;
        self
    }

    async fn remount(&self) {
        // Remove all mocks, then re-register auth-reject + success.
        self.server.reset().await;
        self.mount_auth_reject().await;

        Mock::given(method("POST"))
            .and(path("/v1/models/gemini-pro:streamGenerateContent"))
            .and(BearerTokenMatcher)
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(self.body.clone(), "text/event-stream"),
            )
            .with_priority(1)
            .mount(&self.server)
            .await;
    }

    /// Mount a low-priority mock that returns 401 for requests missing auth.
    async fn mount_auth_reject(&self) {
        Mock::given(method("POST"))
            .and(path("/v1/models/gemini-pro:streamGenerateContent"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(json!({"error": {"message": "missing Bearer token"}})),
            )
            .with_priority(10)
            .mount(&self.server)
            .await;
    }
}

// ===========================================================================
// WireMockBedrock
// ===========================================================================

/// Wiremock stub for AWS Bedrock Converse endpoints.
///
/// Validates `Authorization` header contains `AWS4-HMAC-SHA256` and returns
/// Bedrock-style JSON responses.
pub struct WireMockBedrock {
    server: MockServer,
    response_body: serde_json::Value,
}

impl WireMockBedrock {
    /// Spawn a fresh mock server.
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let stub = Self {
            server,
            response_body: json!({}),
        };
        stub.mount_auth_reject().await;
        stub
    }

    /// Base URL for the mock server.
    pub fn endpoint(&self) -> String {
        self.server.uri()
    }

    /// Configure a canned text response in Bedrock Converse format.
    pub async fn with_response(mut self, text: &str) -> Self {
        self.response_body = json!({
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": text}]
                }
            },
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 10,
                "outputTokens": 5,
                "totalTokens": 15
            }
        });
        self.remount().await;
        self
    }

    /// Configure a tool-use response in Bedrock Converse format.
    pub async fn with_tool_call(mut self, name: &str, args: serde_json::Value) -> Self {
        self.response_body = json!({
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{
                        "toolUse": {
                            "toolUseId": "tool-001",
                            "name": name,
                            "input": args
                        }
                    }]
                }
            },
            "stopReason": "tool_use",
            "usage": {
                "inputTokens": 10,
                "outputTokens": 5,
                "totalTokens": 15
            }
        });
        self.remount().await;
        self
    }

    async fn remount(&self) {
        self.server.reset().await;
        self.mount_auth_reject().await;

        Mock::given(method("POST"))
            .and(path("/model/anthropic.claude-3/converse"))
            .and(AwsSigV4Matcher)
            .and(header_exists("x-amz-date"))
            .respond_with(ResponseTemplate::new(200).set_body_json(self.response_body.clone()))
            .with_priority(1)
            .mount(&self.server)
            .await;
    }

    async fn mount_auth_reject(&self) {
        Mock::given(method("POST"))
            .and(path("/model/anthropic.claude-3/converse"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "message": "Missing Authentication Token",
                "__type": "MissingAuthenticationTokenException"
            })))
            .with_priority(10)
            .mount(&self.server)
            .await;
    }
}

// ===========================================================================
// WireMockAzure
// ===========================================================================

/// Wiremock stub for Azure OpenAI endpoints.
///
/// Validates either `api-key` header or `Authorization: Bearer` header and
/// returns Azure OpenAI-style SSE responses.
pub struct WireMockAzure {
    server: MockServer,
    body: String,
}

impl WireMockAzure {
    /// Spawn a fresh mock server.
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let stub = Self {
            server,
            body: String::new(),
        };
        stub.mount_auth_reject().await;
        stub
    }

    /// Base URL for the mock server.
    pub fn endpoint(&self) -> String {
        self.server.uri()
    }

    /// Configure a canned text response in Azure OpenAI SSE format.
    pub async fn with_response(mut self, text: &str) -> Self {
        let chunk = json!({
            "id": "chatcmpl-azure-001",
            "object": "chat.completion.chunk",
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {"content": text, "role": "assistant"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        self.body
            .push_str(&format!("data: {chunk}\n\ndata: [DONE]\n\n"));
        self.remount().await;
        self
    }

    /// Configure a tool call response in Azure OpenAI SSE format.
    pub async fn with_tool_call(mut self, name: &str, args: serde_json::Value) -> Self {
        let args_str = args.to_string();
        let chunk = json!({
            "id": "chatcmpl-azure-002",
            "object": "chat.completion.chunk",
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_azure_001",
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": args_str
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        self.body
            .push_str(&format!("data: {chunk}\n\ndata: [DONE]\n\n"));
        self.remount().await;
        self
    }

    async fn remount(&self) {
        self.server.reset().await;
        self.mount_auth_reject().await;

        // wiremock path matcher ignores query strings, so callers can append
        // ?api-version=... freely. The stub matches on path alone.
        Mock::given(method("POST"))
            .and(path("/openai/deployments/gpt-4/chat/completions"))
            .and(AzureAuthMatcher)
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(self.body.clone(), "text/event-stream")
                    .append_header("x-ms-region", "East US")
                    .append_header("x-ratelimit-remaining-tokens", "10000"),
            )
            .with_priority(1)
            .mount(&self.server)
            .await;
    }

    async fn mount_auth_reject(&self) {
        Mock::given(method("POST"))
            .and(path("/openai/deployments/gpt-4/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": {
                    "code": "401",
                    "message": "Access denied due to missing api-key or Bearer token."
                }
            })))
            .with_priority(10)
            .mount(&self.server)
            .await;
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex_url(endpoint: &str) -> String {
        format!("{endpoint}/v1/models/gemini-pro:streamGenerateContent")
    }

    fn bedrock_url(endpoint: &str) -> String {
        format!("{endpoint}/model/anthropic.claude-3/converse")
    }

    fn azure_url(endpoint: &str) -> String {
        format!("{endpoint}/openai/deployments/gpt-4/chat/completions")
    }

    // -- Vertex AI -----------------------------------------------------------

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_vertex_wiremock_streams_sse_response() {
        let stub = WireMockVertex::start()
            .await
            .with_response("Hello from Gemini")
            .await;

        let url = vertex_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer test-token-123")
            .json(&json!({"contents": [{"parts": [{"text": "Hi"}]}]}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 200);
        let body = resp.text().await.expect("body");
        assert!(body.contains("Hello from Gemini"), "body: {body}");
        assert!(body.contains("candidates"), "body: {body}");
    }

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_vertex_wiremock_rejects_missing_auth() {
        let stub = WireMockVertex::start()
            .await
            .with_response("Hello from Gemini")
            .await;

        let url = vertex_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&json!({"contents": [{"parts": [{"text": "Hi"}]}]}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 401);
    }

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_vertex_wiremock_tool_call() {
        let stub = WireMockVertex::start()
            .await
            .with_tool_call("read_file", json!({"path": "main.rs"}))
            .await;

        let url = vertex_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer tok")
            .json(&json!({"contents": []}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 200);
        let body = resp.text().await.expect("body");
        assert!(body.contains("functionCall"), "body: {body}");
        assert!(body.contains("read_file"), "body: {body}");
    }

    // -- Bedrock -------------------------------------------------------------

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_bedrock_wiremock_returns_response() {
        let stub = WireMockBedrock::start()
            .await
            .with_response("Hello from Bedrock")
            .await;

        let url = bedrock_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .header(
                "Authorization",
                "AWS4-HMAC-SHA256 Credential=AKID/20260501/us-east-1/bedrock/aws4_request",
            )
            .header("x-amz-date", "20260501T000000Z")
            .json(&json!({"messages": [{"role": "user", "content": [{"text": "Hi"}]}]}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json body");
        let text = body["output"]["message"]["content"][0]["text"]
            .as_str()
            .expect("text field");
        assert_eq!(text, "Hello from Bedrock");
    }

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_bedrock_wiremock_rejects_missing_auth() {
        let stub = WireMockBedrock::start()
            .await
            .with_response("Hello from Bedrock")
            .await;

        let url = bedrock_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&json!({"messages": []}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 403);
    }

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_bedrock_wiremock_tool_call() {
        let stub = WireMockBedrock::start()
            .await
            .with_tool_call("write_file", json!({"path": "out.txt", "content": "hi"}))
            .await;

        let url = bedrock_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .header(
                "Authorization",
                "AWS4-HMAC-SHA256 Credential=AKID/20260501/us-east-1/bedrock/aws4_request",
            )
            .header("x-amz-date", "20260501T000000Z")
            .json(&json!({"messages": []}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json body");
        let tool_use = &body["output"]["message"]["content"][0]["toolUse"];
        assert_eq!(tool_use["name"].as_str(), Some("write_file"));
    }

    // -- Azure OpenAI --------------------------------------------------------

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_azure_wiremock_accepts_api_key() {
        let stub = WireMockAzure::start()
            .await
            .with_response("Hello from Azure")
            .await;

        let url = azure_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .header("api-key", "my-azure-key-123")
            .json(&json!({"messages": [{"role": "user", "content": "Hi"}]}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 200);
        let body = resp.text().await.expect("body");
        assert!(body.contains("Hello from Azure"), "body: {body}");
    }

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_azure_wiremock_accepts_bearer() {
        let stub = WireMockAzure::start()
            .await
            .with_response("Hello from Azure")
            .await;

        let url = azure_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer azure-ad-token-456")
            .json(&json!({"messages": [{"role": "user", "content": "Hi"}]}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 200);
        let body = resp.text().await.expect("body");
        assert!(body.contains("Hello from Azure"), "body: {body}");
    }

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_azure_wiremock_rejects_missing_auth() {
        let stub = WireMockAzure::start()
            .await
            .with_response("Hello from Azure")
            .await;

        let url = azure_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&json!({"messages": [{"role": "user", "content": "Hi"}]}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 401);
    }

    // rtmx:req REQ-TEST-045
    #[tokio::test]
    async fn test_azure_wiremock_tool_call() {
        let stub = WireMockAzure::start()
            .await
            .with_tool_call("search", json!({"query": "test"}))
            .await;

        let url = azure_url(&stub.endpoint());
        let resp = reqwest::Client::new()
            .post(&url)
            .header("api-key", "key")
            .json(&json!({"messages": []}))
            .send()
            .await
            .expect("request succeeds");

        assert_eq!(resp.status(), 200);
        let body = resp.text().await.expect("body");
        assert!(body.contains("tool_calls"), "body: {body}");
        assert!(body.contains("search"), "body: {body}");
    }
}
