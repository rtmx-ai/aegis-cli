//! /connect command: parse provider arguments and build a ConnectRequest.
//!
//! The TUI crate does not depend on aegis-llm or aegis-onboard. Instead,
//! it produces a `ConnectRequest` value-object that the composition root
//! (aegis-cli/src/main.rs) translates into a `ProviderConfig`, validates
//! auth, probes the endpoint, saves to config.yaml, and swaps the live
//! provider. This keeps bounded-context boundaries clean.

use std::collections::HashMap;

/// A provider kind as understood by the /connect command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectProvider {
    /// Local OpenAI-compatible endpoint (Ollama, vLLM, TGI).
    Local,
    /// Google Vertex AI.
    Vertex,
    /// AWS Bedrock.
    Bedrock,
    /// Azure OpenAI.
    Azure,
}

/// A parsed /connect request from the TUI. The composition root converts
/// this into a `ProviderConfig`, resolves auth, and probes the endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRequest {
    pub provider: ConnectProvider,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub project: Option<String>,
    pub region: Option<String>,
}

impl ConnectRequest {
    /// Build from a local URL shorthand: `/connect http://localhost:11434/v1`
    pub fn local_url(url: &str) -> Self {
        Self {
            provider: ConnectProvider::Local,
            endpoint: Some(url.to_string()),
            model: None,
            project: None,
            region: None,
        }
    }

    /// Build an explicit local request: `/connect local [url]`
    pub fn local(url: Option<&str>) -> Self {
        Self {
            provider: ConnectProvider::Local,
            endpoint: url.map(|u| u.to_string()),
            model: None,
            project: None,
            region: None,
        }
    }
}

/// Parse /connect arguments into a ConnectRequest.
///
/// Accepted forms:
/// - `/connect http://localhost:11434/v1` -- bare URL -> local
/// - `/connect local http://...` -- explicit local with optional URL
/// - `/connect vertex --project=X --region=Y --model=Z`
/// - `/connect bedrock --region=X --model=Z`
/// - `/connect azure --endpoint=X --model=Z`
///
/// Returns `Err(user_facing_message)` for unrecognized providers.
pub fn parse_connect_args(args: &str) -> Result<ConnectRequest, String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        return Err(String::new()); // no args = show current
    }

    let provider_or_url = parts[0];

    // Bare URL shorthand
    if provider_or_url.starts_with("http://") || provider_or_url.starts_with("https://") {
        return Ok(ConnectRequest::local_url(provider_or_url));
    }

    // Parse --key=value flags from remaining args
    let flags = parse_flags(&parts[1..]);

    match provider_or_url.to_lowercase().as_str() {
        "local" => {
            // /connect local [url]
            // The URL can be a positional arg or --endpoint=
            let url = parts
                .get(1)
                .filter(|s| s.starts_with("http"))
                .copied()
                .or(flags.get("endpoint").map(|s| s.as_str()));
            Ok(ConnectRequest {
                provider: ConnectProvider::Local,
                endpoint: url.map(|u| u.to_string()),
                model: flags.get("model").cloned(),
                project: None,
                region: None,
            })
        }
        "vertex" => Ok(ConnectRequest {
            provider: ConnectProvider::Vertex,
            endpoint: flags.get("endpoint").cloned(),
            model: flags.get("model").cloned(),
            project: flags.get("project").cloned(),
            region: flags.get("region").cloned(),
        }),
        "bedrock" => Ok(ConnectRequest {
            provider: ConnectProvider::Bedrock,
            endpoint: flags.get("endpoint").cloned(),
            model: flags.get("model").cloned(),
            project: None,
            region: flags.get("region").cloned(),
        }),
        "azure" => {
            let endpoint = flags.get("endpoint").cloned().or_else(|| {
                // Allow positional: /connect azure https://...
                parts
                    .get(1)
                    .filter(|s| s.starts_with("http"))
                    .map(|s| s.to_string())
            });
            if endpoint.is_none() {
                return Err("Azure requires an endpoint URL:\n\
                     /connect azure --endpoint=https://myresource.openai.azure.com"
                    .to_string());
            }
            Ok(ConnectRequest {
                provider: ConnectProvider::Azure,
                endpoint,
                model: flags.get("model").cloned(),
                project: None,
                region: flags.get("region").cloned(),
            })
        }
        other => Err(format!(
            "Unknown provider '{other}'.\n\
             Options: local, vertex, bedrock, azure\n\
             Or: /connect http://... for direct endpoint"
        )),
    }
}

/// Parse `--key=value` flags from a slice of argument tokens.
fn parse_flags(tokens: &[&str]) -> HashMap<String, String> {
    let mut flags = HashMap::new();
    for token in tokens {
        if let Some(kv) = token.strip_prefix("--")
            && let Some((key, value)) = kv.split_once('=')
        {
            flags.insert(key.to_string(), value.to_string());
        }
    }
    flags
}

/// Auth guidance message for a failed cloud provider connection.
pub fn auth_guidance(provider: &ConnectProvider) -> &'static str {
    match provider {
        ConnectProvider::Local => "Check that the endpoint is running and accessible.",
        ConnectProvider::Vertex => {
            "GCP Application Default Credentials not found.\n\
             Run: gcloud auth application-default login"
        }
        ConnectProvider::Bedrock => {
            "AWS credentials not found.\n\
             Set: AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY\n\
             Or: aws configure sso"
        }
        ConnectProvider::Azure => {
            "Azure credentials not found.\n\
             Set: AZURE_TENANT_ID + AZURE_CLIENT_ID\n\
             Or: AZURE_OPENAI_API_KEY for key-based auth"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-027
    #[test]
    fn test_parse_connect_bare_url() {
        let req = parse_connect_args("http://localhost:11434/v1").unwrap();
        assert_eq!(req.provider, ConnectProvider::Local);
        assert_eq!(req.endpoint.as_deref(), Some("http://localhost:11434/v1"));
    }

    // rtmx:req REQ-LLM-027
    #[test]
    fn test_parse_connect_https_url() {
        let req = parse_connect_args("https://my-vllm:8080/v1").unwrap();
        assert_eq!(req.provider, ConnectProvider::Local);
        assert_eq!(req.endpoint.as_deref(), Some("https://my-vllm:8080/v1"));
    }

    // rtmx:req REQ-LLM-027
    #[test]
    fn test_parse_connect_no_args() {
        let result = parse_connect_args("");
        assert!(result.is_err());
        assert!(result.unwrap_err().is_empty()); // empty = show current
    }

    // rtmx:req REQ-LLM-027
    #[test]
    fn test_parse_connect_local_explicit() {
        let req = parse_connect_args("local").unwrap();
        assert_eq!(req.provider, ConnectProvider::Local);
        assert!(req.endpoint.is_none());
    }

    // rtmx:req REQ-LLM-027
    #[test]
    fn test_parse_connect_local_with_url() {
        let req = parse_connect_args("local http://localhost:8080/v1").unwrap();
        assert_eq!(req.provider, ConnectProvider::Local);
        assert_eq!(req.endpoint.as_deref(), Some("http://localhost:8080/v1"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_vertex_with_project_region_model() {
        let req = parse_connect_args(
            "vertex --project=aegis-cli-demo --region=us-central1 \
             --model=google/gemini-2.5-pro",
        )
        .unwrap();
        assert_eq!(req.provider, ConnectProvider::Vertex);
        assert_eq!(req.project.as_deref(), Some("aegis-cli-demo"));
        assert_eq!(req.region.as_deref(), Some("us-central1"));
        assert_eq!(req.model.as_deref(), Some("google/gemini-2.5-pro"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_bedrock_with_region() {
        let req = parse_connect_args(
            "bedrock --region=us-gov-west-1 \
             --model=claude-3-sonnet-20241022",
        )
        .unwrap();
        assert_eq!(req.provider, ConnectProvider::Bedrock);
        assert_eq!(req.region.as_deref(), Some("us-gov-west-1"));
        assert_eq!(req.model.as_deref(), Some("claude-3-sonnet-20241022"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_azure_with_endpoint_model() {
        let req = parse_connect_args(
            "azure --endpoint=https://myendpoint.openai.azure.com \
             --model=gpt-4o",
        )
        .unwrap();
        assert_eq!(req.provider, ConnectProvider::Azure);
        assert_eq!(
            req.endpoint.as_deref(),
            Some("https://myendpoint.openai.azure.com")
        );
        assert_eq!(req.model.as_deref(), Some("gpt-4o"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_azure_positional_endpoint() {
        let req = parse_connect_args("azure https://myendpoint.openai.azure.com").unwrap();
        assert_eq!(req.provider, ConnectProvider::Azure);
        assert_eq!(
            req.endpoint.as_deref(),
            Some("https://myendpoint.openai.azure.com")
        );
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_azure_requires_endpoint() {
        let result = parse_connect_args("azure");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("endpoint"));
    }

    // rtmx:req REQ-LLM-027
    #[test]
    fn test_parse_connect_unknown_provider() {
        let result = parse_connect_args("openai");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown provider"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_vertex_minimal() {
        let req = parse_connect_args("vertex").unwrap();
        assert_eq!(req.provider, ConnectProvider::Vertex);
        assert!(req.project.is_none());
        assert!(req.region.is_none());
        assert!(req.model.is_none());
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_bedrock_minimal() {
        let req = parse_connect_args("bedrock").unwrap();
        assert_eq!(req.provider, ConnectProvider::Bedrock);
        assert!(req.region.is_none());
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_auth_guidance_vertex() {
        let msg = auth_guidance(&ConnectProvider::Vertex);
        assert!(msg.contains("gcloud auth"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_auth_guidance_bedrock() {
        let msg = auth_guidance(&ConnectProvider::Bedrock);
        assert!(msg.contains("AWS_ACCESS_KEY_ID"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_auth_guidance_azure() {
        let msg = auth_guidance(&ConnectProvider::Azure);
        assert!(msg.contains("AZURE_TENANT_ID"));
    }

    // rtmx:req REQ-LLM-027
    #[test]
    fn test_auth_guidance_local() {
        let msg = auth_guidance(&ConnectProvider::Local);
        assert!(msg.contains("endpoint"));
    }

    // rtmx:req REQ-LLM-029
    #[test]
    fn test_parse_connect_case_insensitive() {
        let req = parse_connect_args("VERTEX --project=test").unwrap();
        assert_eq!(req.provider, ConnectProvider::Vertex);
        assert_eq!(req.project.as_deref(), Some("test"));
    }

    // rtmx:req REQ-TUI-063d
    #[test]
    fn test_connect_guided_flow_vertex() {
        let req = parse_connect_args(
            "vertex --model=gemini-3.1-pro --region=us-central1 --project=my-proj",
        )
        .unwrap();
        assert_eq!(req.provider, ConnectProvider::Vertex);
        assert_eq!(req.model.as_deref(), Some("gemini-3.1-pro"));
        assert_eq!(req.region.as_deref(), Some("us-central1"));
        assert_eq!(req.project.as_deref(), Some("my-proj"));
    }

    // rtmx:req REQ-TUI-063d
    #[test]
    fn test_connect_guided_flow_bedrock() {
        let req =
            parse_connect_args("bedrock --model=claude-opus-sonnet-4.5 --region=us-gov-west-1")
                .unwrap();
        assert_eq!(req.provider, ConnectProvider::Bedrock);
        assert_eq!(req.model.as_deref(), Some("claude-opus-sonnet-4.5"));
        assert_eq!(req.region.as_deref(), Some("us-gov-west-1"));
    }

    // rtmx:req REQ-TUI-063d
    #[test]
    fn test_connect_guided_flow_local() {
        let req = parse_connect_args("local --model=llama3").unwrap();
        assert_eq!(req.provider, ConnectProvider::Local);
        assert_eq!(req.model.as_deref(), Some("llama3"));
    }

    // rtmx:req REQ-TUI-063d
    #[test]
    fn test_connect_guided_flow_azure() {
        let req = parse_connect_args(
            "azure --model=gpt-5.1 --region=usgovvirginia \
             --endpoint=https://myendpoint.openai.azure.com",
        )
        .unwrap();
        assert_eq!(req.provider, ConnectProvider::Azure);
        assert_eq!(req.model.as_deref(), Some("gpt-5.1"));
        assert_eq!(req.region.as_deref(), Some("usgovvirginia"));
        assert_eq!(
            req.endpoint.as_deref(),
            Some("https://myendpoint.openai.azure.com")
        );
    }

    // rtmx:req REQ-TUI-063d
    #[test]
    fn test_connect_grammar_builds_parseable_string() {
        use crate::command_palette::{connect_grammar, options_for_provider};

        let grammar = connect_grammar();
        // Pick the first provider (vertex)
        let provider = "vertex";
        let model_opts = options_for_provider(provider, "model");
        let region_opts = options_for_provider(provider, "region");

        assert!(!model_opts.is_empty(), "vertex should have model options");
        assert!(!region_opts.is_empty(), "vertex should have region options");

        // Build a token string the way the guided palette would
        let mut tokens = String::new();
        tokens.push_str(provider);
        // Append model with its prefix
        let model_prefix = grammar.slots[1].prefix.as_deref().unwrap_or("");
        tokens.push_str(&format!(" {}{}", model_prefix, model_opts[0].value));
        // Append region with its prefix
        let region_prefix = grammar.slots[2].prefix.as_deref().unwrap_or("");
        tokens.push_str(&format!(" {}{}", region_prefix, region_opts[0].value));

        let req = parse_connect_args(&tokens).unwrap();
        assert_eq!(req.provider, ConnectProvider::Vertex);
        assert_eq!(req.model.as_deref(), Some(model_opts[0].value.as_str()));
        assert_eq!(req.region.as_deref(), Some(region_opts[0].value.as_str()));
    }

    // rtmx:req REQ-TUI-063d
    #[test]
    fn test_connect_with_project_id() {
        let req = parse_connect_args(
            "vertex --model=claude-sonnet-4.6 --region=us-east4 \
             --project=aegis-prod-il5",
        )
        .unwrap();
        assert_eq!(req.provider, ConnectProvider::Vertex);
        assert_eq!(req.model.as_deref(), Some("claude-sonnet-4.6"));
        assert_eq!(req.region.as_deref(), Some("us-east4"));
        assert_eq!(req.project.as_deref(), Some("aegis-prod-il5"));
    }
}
