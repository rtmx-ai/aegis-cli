//! AWS Bedrock provider via the Converse Stream API.
//!
//! Connects to AWS Bedrock using SigV4 request signing and the Converse
//! streaming API. Supports commercial and GovCloud regions. Uses lightweight
//! aws-sigv4 crate instead of the full SDK to minimize transitive deps.

use std::sync::Arc;

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;
use reqwest::Client;

use crate::auth::ProviderAuth;
use crate::bedrock_stream::BedrockTokenStream;
use crate::config::ProviderConfig;

/// Default maximum idle connections per host for connection pooling (REQ-LLM-019).
const POOL_MAX_IDLE_PER_HOST: usize = 4;

/// Provider that speaks to AWS Bedrock via the Converse Stream API.
#[derive(Debug)]
pub struct BedrockProvider {
    client: Arc<Client>,
    endpoint_url: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
    region: String,
}

impl BedrockProvider {
    /// Create a new BedrockProvider from config and pre-resolved AWS credentials.
    ///
    /// Region is required (from auth). The endpoint URL is constructed as:
    /// `https://bedrock-runtime.{region}.amazonaws.com/model/{model}/converse-stream`
    pub fn new(config: &ProviderConfig, auth: ProviderAuth) -> Result<Self, DomainError> {
        let (access_key_id, secret_access_key, session_token, region) = match auth {
            ProviderAuth::Aws {
                access_key_id,
                secret_access_key,
                session_token,
                region,
            } => (access_key_id, secret_access_key, session_token, region),
            _ => {
                return Err(DomainError::ConfigError {
                    message: "Bedrock provider requires AWS auth credentials".to_string(),
                });
            }
        };

        if region.trim().is_empty() {
            return Err(DomainError::ConfigError {
                message: "Bedrock provider requires a non-empty AWS region".to_string(),
            });
        }

        let endpoint_url = format!(
            "https://bedrock-runtime.{region}.amazonaws.com\
             /model/{model}/converse-stream",
            region = region,
            model = config.model,
        );

        let client = Arc::new(
            Client::builder()
                .connect_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
                .timeout(std::time::Duration::from_secs(config.read_timeout_secs))
                .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
                .build()
                .map_err(|e| DomainError::ProviderError {
                    message: format!("Failed to create HTTP client: {e}"),
                })?,
        );

        tracing::info!(
            provider = "bedrock",
            model = %config.model,
            endpoint = %endpoint_url,
            region = %region,
            "provider initialized"
        );

        Ok(Self {
            client,
            endpoint_url,
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            access_key_id,
            secret_access_key,
            session_token,
            region,
        })
    }

    /// Return a reference to the Arc-wrapped HTTP client (REQ-LLM-019).
    pub fn shared_client(&self) -> &Arc<Client> {
        &self.client
    }

    /// Build the Bedrock Converse API request body.
    ///
    /// Bedrock uses a distinct format from OpenAI: text is wrapped in
    /// `{"text": "..."}` content blocks, system prompts go in a top-level
    /// `"system"` field, and tools use `"toolConfig"` with `"toolSpec"` wrappers.
    pub(crate) fn build_converse_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> serde_json::Value {
        let mut system_prompts: Vec<serde_json::Value> = Vec::new();
        let mut converse_messages: Vec<serde_json::Value> = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    let mut block = serde_json::json!({
                        "text": msg.content,
                    });
                    if let Some(ref cc) = msg.cache_control {
                        block["cachePoint"] = serde_json::json!({"type": cc});
                    }
                    system_prompts.push(block);
                }
                _ => {
                    let role_str = match msg.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "user",
                        Role::System => unreachable!(),
                    };
                    let mut content_block = serde_json::json!({"text": msg.content});
                    if let Some(ref cc) = msg.cache_control {
                        content_block["cachePoint"] = serde_json::json!({"type": cc});
                    }
                    converse_messages.push(serde_json::json!({
                        "role": role_str,
                        "content": [content_block],
                    }));
                }
            }
        }

        let mut body = serde_json::json!({
            "messages": converse_messages,
            "inferenceConfig": {
                "maxTokens": self.max_tokens,
                "temperature": self.temperature,
            },
        });

        if !system_prompts.is_empty() {
            body["system"] = serde_json::Value::Array(system_prompts);
        }

        if !tools.is_empty() {
            let tool_specs: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "toolSpec": {
                            "name": t.name,
                            "description": t.description,
                            "inputSchema": {
                                "json": t.parameters,
                            }
                        }
                    })
                })
                .collect();
            body["toolConfig"] = serde_json::json!({
                "tools": tool_specs,
            });
        }

        body
    }

    /// Compute SigV4 signature headers for an AWS request.
    ///
    /// Returns a list of (header_name, header_value) pairs that must be added
    /// to the HTTP request: Authorization, x-amz-date, x-amz-content-sha256,
    /// and optionally x-amz-security-token.
    pub(crate) fn sign_request(
        &self,
        method: &str,
        url: &str,
        body: &[u8],
    ) -> Result<Vec<(String, String)>, DomainError> {
        self.sign_request_at(method, url, body, std::time::SystemTime::now())
    }

    /// Sign a request at a specific point in time (for testability).
    fn sign_request_at(
        &self,
        method: &str,
        url: &str,
        body: &[u8],
        time: std::time::SystemTime,
    ) -> Result<Vec<(String, String)>, DomainError> {
        use aws_credential_types::Credentials;
        use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
        use aws_sigv4::sign::v4;
        use aws_smithy_runtime_api::client::identity::Identity;

        let credentials = Credentials::new(
            &self.access_key_id,
            &self.secret_access_key,
            self.session_token.clone(),
            None,
            "aegis-bedrock",
        );

        let identity = Identity::from(credentials);
        let mut settings = SigningSettings::default();
        settings.payload_checksum_kind = aws_sigv4::http_request::PayloadChecksumKind::XAmzSha256;

        let v4_params = v4::SigningParams::builder()
            .identity(&identity)
            .region(&self.region)
            .name("bedrock")
            .time(time)
            .settings(settings)
            .build()
            .map_err(|e| DomainError::ProviderError {
                message: format!("Failed to build SigV4 signing params: {e}"),
            })?;

        let signable_request = SignableRequest::new(
            method,
            url,
            std::iter::once(("content-type", "application/json")),
            SignableBody::Bytes(body),
        )
        .map_err(|e| DomainError::ProviderError {
            message: format!("Failed to create signable request: {e}"),
        })?;

        let signing_params = aws_sigv4::http_request::SigningParams::from(v4_params);

        let (signing_instructions, _signature) = sign(signable_request, &signing_params)
            .map_err(|e| DomainError::ProviderError {
                message: format!("SigV4 signing failed: {e}"),
            })?
            .into_parts();

        let mut headers = Vec::new();
        for (name, value) in signing_instructions.headers() {
            headers.push((
                name.to_string(),
                String::from_utf8(value.as_bytes().to_vec()).map_err(|e| {
                    DomainError::ProviderError {
                        message: format!("Non-UTF8 signing header value: {e}"),
                    }
                })?,
            ));
        }

        Ok(headers)
    }
}

#[async_trait]
impl LlmProvider for BedrockProvider {
    async fn health_check(&self) -> ProviderHealth {
        // For Bedrock, we verify the endpoint is reachable by sending a
        // minimal request. A 403 means valid creds but no model access,
        // which still means the endpoint is reachable. Only connection
        // errors indicate an unhealthy state.
        let health_client = match Client::builder()
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

        let health_url = format!("https://bedrock-runtime.{}.amazonaws.com/", self.region);

        let start = std::time::Instant::now();

        // Build a minimal body and sign it
        let body = b"{}";
        let sign_headers = match self.sign_request("GET", &health_url, body) {
            Ok(h) => h,
            Err(e) => {
                return ProviderHealth::Unhealthy {
                    message: format!("Failed to sign health check request: {e}"),
                };
            }
        };

        let mut req = health_client.get(&health_url);
        for (name, value) in &sign_headers {
            req = req.header(name.as_str(), value.as_str());
        }

        match req.send().await {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let status = response.status().as_u16();
                // 403 = valid endpoint, creds work but no permission for
                // this specific action. Still counts as reachable.
                if status == 403 || response.status().is_success() {
                    if latency_ms < 1000 {
                        ProviderHealth::Healthy { latency_ms }
                    } else {
                        ProviderHealth::Degraded {
                            latency_ms,
                            message: format!(
                                "Response took {latency_ms}ms \
                                 (> 1000ms threshold)"
                            ),
                        }
                    }
                } else {
                    ProviderHealth::Unhealthy {
                        message: format!("Health check returned HTTP {status}"),
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
        let body = self.build_converse_body(messages, tools);
        let body_bytes = serde_json::to_vec(&body).map_err(|e| DomainError::ProviderError {
            message: format!("Failed to serialize request body: {e}"),
        })?;

        tracing::debug!(
            model = %self.model,
            messages = messages.len(),
            tools = tools.len(),
            "starting Bedrock Converse stream"
        );

        let sign_headers = self.sign_request("POST", &self.endpoint_url, &body_bytes)?;

        let mut req = self
            .client
            .post(&self.endpoint_url)
            .header("content-type", "application/json");

        for (name, value) in &sign_headers {
            req = req.header(name.as_str(), value.as_str());
        }

        let response =
            req.body(body_bytes)
                .send()
                .await
                .map_err(|e| DomainError::ProviderError {
                    message: format!("Request to {} failed: {e}", self.endpoint_url),
                })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::ProviderError {
                message: format!("Bedrock API returned {status}: {body}"),
            });
        }

        let bytes_stream = response.bytes_stream();
        Ok(Box::new(BedrockTokenStream::new(bytes_stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ProviderKind};

    fn bedrock_config() -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::Bedrock,
            model: "us.anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
            endpoint: "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
            connect_timeout_secs: 10,
            read_timeout_secs: 300,
            project_id: None,
            region: Some("us-east-1".to_string()),
        }
    }

    fn test_aws_auth(region: &str) -> ProviderAuth {
        ProviderAuth::Aws {
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: None,
            region: region.to_string(),
        }
    }

    fn test_aws_auth_with_session(region: &str) -> ProviderAuth {
        ProviderAuth::Aws {
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: Some("FwoGZXIvYXdzEBAaD...".to_string()),
            region: region.to_string(),
        }
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn new_fails_without_region() {
        let cfg = bedrock_config();
        let auth = ProviderAuth::Aws {
            access_key_id: "AKIA".to_string(),
            secret_access_key: "secret".to_string(),
            session_token: None,
            region: "".to_string(),
        };
        let result = BedrockProvider::new(&cfg, auth);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("region"), "Error should mention region: {err}");
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn new_fails_with_wrong_auth_variant() {
        let cfg = bedrock_config();
        let auth = ProviderAuth::NoAuth;
        let result = BedrockProvider::new(&cfg, auth);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("AWS"), "Error should mention AWS: {err}");
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn new_builds_commercial_endpoint() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();
        assert_eq!(
            provider.endpoint_url,
            "https://bedrock-runtime.us-east-1.amazonaws.com\
             /model/us.anthropic.claude-3-5-sonnet-20241022-v2:0\
             /converse-stream"
        );
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn new_builds_govcloud_endpoint() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-gov-west-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();
        assert_eq!(
            provider.endpoint_url,
            "https://bedrock-runtime.us-gov-west-1.amazonaws.com\
             /model/us.anthropic.claude-3-5-sonnet-20241022-v2:0\
             /converse-stream"
        );
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn converse_body_structure() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let messages = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
            cache_control: None,
        }];

        let body = provider.build_converse_body(&messages, &[]);

        // Must have messages array
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        // Content is wrapped in text blocks
        assert_eq!(msgs[0]["content"][0]["text"], "Hello");

        // Must have inferenceConfig
        assert_eq!(body["inferenceConfig"]["maxTokens"], 4096);
        assert_eq!(body["inferenceConfig"]["temperature"], 0.0);

        // No system field when no system messages
        assert!(body.get("system").is_none());
        // No toolConfig when no tools
        assert!(body.get("toolConfig").is_none());
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn converse_body_includes_system_prompt() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let messages = vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant.".to_string(),
                cache_control: None,
            },
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                cache_control: None,
            },
        ];

        let body = provider.build_converse_body(&messages, &[]);

        // System prompt should be in the top-level "system" field
        let system = body["system"].as_array().unwrap();
        assert_eq!(system.len(), 1);
        assert_eq!(system[0]["text"], "You are a helpful assistant.");

        // System messages should NOT appear in the messages array
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn converse_body_includes_tools() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let tools = vec![ToolSchema {
            name: "read_file".to_string(),
            description: "Read a file from disk".to_string(),
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
            cache_control: None,
        }];

        let body = provider.build_converse_body(&messages, &tools);

        let tool_config = &body["toolConfig"];
        let tools_arr = tool_config["tools"].as_array().unwrap();
        assert_eq!(tools_arr.len(), 1);
        assert_eq!(tools_arr[0]["toolSpec"]["name"], "read_file");
        assert_eq!(
            tools_arr[0]["toolSpec"]["description"],
            "Read a file from disk"
        );
        assert_eq!(
            tools_arr[0]["toolSpec"]["inputSchema"]["json"]["type"],
            "object"
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn new_fails_with_empty_model() {
        let mut cfg = bedrock_config();
        cfg.model = "".to_string();
        let auth = test_aws_auth("us-east-1");
        // An empty model produces a malformed endpoint URL. The provider
        // constructor succeeds but the endpoint is useless. Verify it does
        // not panic and the endpoint contains the empty model segment.
        let provider = BedrockProvider::new(&cfg, auth).unwrap();
        assert!(
            provider.endpoint_url.contains("/model//converse-stream"),
            "Endpoint should contain empty model segment: {}",
            provider.endpoint_url
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn new_fails_with_empty_region() {
        let cfg = bedrock_config();
        let auth = ProviderAuth::Aws {
            access_key_id: "AKIA".to_string(),
            secret_access_key: "secret".to_string(),
            session_token: None,
            region: "   ".to_string(), // whitespace-only, not just ""
        };
        let result = BedrockProvider::new(&cfg, auth);
        assert!(result.is_err(), "Whitespace-only region should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("region"), "Error should mention region: {err}");
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn converse_body_handles_empty_messages() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let body = provider.build_converse_body(&[], &[]);

        // Should produce valid JSON with empty messages array
        let msgs = body["messages"].as_array().unwrap();
        assert!(msgs.is_empty(), "messages should be empty");
        assert!(
            body.get("system").is_none(),
            "system field should be absent"
        );
        assert!(
            body.get("toolConfig").is_none(),
            "toolConfig should be absent"
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn converse_body_handles_empty_tools() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let messages = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
            cache_control: None,
        }];

        let body = provider.build_converse_body(&messages, &[]);

        assert!(
            body.get("toolConfig").is_none(),
            "toolConfig should be omitted when tools is empty"
        );
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn sign_request_adds_auth_headers() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let body = b"{}";
        let headers = provider
            .sign_request("POST", &provider.endpoint_url, body)
            .unwrap();

        let header_names: Vec<&str> = headers.iter().map(|(n, _)| n.as_str()).collect();

        assert!(
            header_names.contains(&"authorization"),
            "Must have authorization header, got: {:?}",
            header_names
        );
        assert!(
            header_names.contains(&"x-amz-date"),
            "Must have x-amz-date header, got: {:?}",
            header_names
        );
    }

    // rtmx:req REQ-LLM-002
    #[test]
    fn sign_request_includes_session_token() {
        let cfg = bedrock_config();
        let auth = test_aws_auth_with_session("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let body = b"{}";
        let headers = provider
            .sign_request("POST", &provider.endpoint_url, body)
            .unwrap();

        let header_names: Vec<&str> = headers.iter().map(|(n, _)| n.as_str()).collect();

        assert!(
            header_names.contains(&"x-amz-security-token"),
            "Must have x-amz-security-token when session_token provided, \
             got: {:?}",
            header_names
        );
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn converse_body_includes_cache_control_on_system() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let messages = vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant.".to_string(),
                cache_control: Some("ephemeral".to_string()),
            },
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                cache_control: None,
            },
        ];

        let body = provider.build_converse_body(&messages, &[]);
        let system = body["system"].as_array().unwrap();
        assert_eq!(system[0]["cachePoint"]["type"], "ephemeral");

        // User message should not have cachePoint
        let msgs = body["messages"].as_array().unwrap();
        assert!(
            msgs[0]["content"][0].get("cachePoint").is_none(),
            "user message without cache_control should not have cachePoint"
        );
    }

    // rtmx:req REQ-LLM-014
    #[test]
    fn converse_body_omits_cache_control_when_none() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();

        let messages = vec![Message {
            role: Role::System,
            content: "You are a helpful assistant.".to_string(),
            cache_control: None,
        }];

        let body = provider.build_converse_body(&messages, &[]);
        let system = body["system"].as_array().unwrap();
        assert!(
            system[0].get("cachePoint").is_none(),
            "cachePoint should be absent when cache_control is None"
        );
    }

    // rtmx:req REQ-LLM-019
    #[test]
    fn client_is_arc_shared() {
        let cfg = bedrock_config();
        let auth = test_aws_auth("us-east-1");
        let provider = BedrockProvider::new(&cfg, auth).unwrap();
        let arc1 = provider.shared_client().clone();
        let arc2 = provider.shared_client().clone();
        assert!(Arc::ptr_eq(&arc1, &arc2));
    }
}
