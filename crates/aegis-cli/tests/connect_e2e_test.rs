// End-to-end tests for the /connect pipeline:
//   ConnectRequest (TUI) -> ProviderConfig (aegis-llm) -> provider factory
//
// These tests verify the full integration path without requiring live
// cloud credentials. They exercise type conversions, config construction,
// and provider instantiation for each supported backend.

use aegis_llm::config::{ProviderConfig, ProviderKind};
use aegis_llm::provider::{create_provider, create_vertex_provider_with_token};
use aegis_tui::app::{ConnectProvider, ConnectRequest};

// ---------------------------------------------------------------------------
// Local provider pipeline
// ---------------------------------------------------------------------------

// rtmx:req REQ-TEST-044
#[test]
fn connect_local_url_produces_valid_provider_config() {
    let req = ConnectRequest::local_url("http://localhost:11434/v1");
    assert_eq!(req.provider, ConnectProvider::Local);
    assert_eq!(req.endpoint.as_deref(), Some("http://localhost:11434/v1"));

    // Convert to ProviderConfig the same way the composition root would.
    let endpoint = req
        .endpoint
        .as_deref()
        .unwrap_or("http://localhost:11434/v1");
    let model = req.model.as_deref().unwrap_or("llama3");
    let cfg = ProviderConfig::local(endpoint, model);

    assert_eq!(cfg.kind, ProviderKind::Local);
    assert_eq!(cfg.endpoint, "http://localhost:11434/v1");
    assert_eq!(cfg.model, "llama3");
}

// rtmx:req REQ-TEST-044
#[test]
fn connect_local_no_url_uses_default_endpoint() {
    let req = ConnectRequest::local(None);
    assert_eq!(req.provider, ConnectProvider::Local);
    assert!(req.endpoint.is_none());

    let endpoint = req
        .endpoint
        .as_deref()
        .unwrap_or("http://localhost:11434/v1");
    let cfg = ProviderConfig::local(endpoint, "llama3");
    assert_eq!(cfg.endpoint, "http://localhost:11434/v1");
}

// rtmx:req REQ-TEST-044
#[test]
fn connect_local_creates_working_provider() {
    let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
    let result = create_provider(&cfg);
    assert!(result.is_ok(), "local provider factory should succeed");
}

// ---------------------------------------------------------------------------
// Vertex AI pipeline
// ---------------------------------------------------------------------------

// rtmx:req REQ-TEST-044
#[test]
fn connect_vertex_request_maps_to_provider_config() {
    let req = ConnectRequest {
        provider: ConnectProvider::Vertex,
        endpoint: None,
        model: Some("gemini-2.5-pro-001".to_string()),
        project: Some("aegis-demo".to_string()),
        region: Some("us-central1".to_string()),
    };

    let project = req.project.as_deref().unwrap_or("my-project");
    let region = req.region.as_deref().unwrap_or("us-central1");
    let model = req.model.as_deref().unwrap_or("gemini-2.5-pro-001");
    let cfg = ProviderConfig::vertex(project, region, model);

    assert_eq!(cfg.kind, ProviderKind::Vertex);
    assert_eq!(cfg.project_id.as_deref(), Some("aegis-demo"));
    assert_eq!(cfg.region.as_deref(), Some("us-central1"));
    assert!(cfg.endpoint.contains("aegis-demo"));
    assert!(cfg.endpoint.contains("us-central1"));
}

// rtmx:req REQ-TEST-044
#[test]
fn connect_vertex_with_token_creates_provider() {
    let cfg = ProviderConfig::vertex("test-project", "us-central1", "gemini-2.5-pro-001");
    let result = create_vertex_provider_with_token(&cfg, "ya29.fake-token".into());
    assert!(
        result.is_ok(),
        "vertex provider with pre-resolved token should succeed"
    );
}

// ---------------------------------------------------------------------------
// Bedrock pipeline
// ---------------------------------------------------------------------------

// rtmx:req REQ-TEST-044
#[test]
fn connect_bedrock_request_maps_to_provider_config() {
    let req = ConnectRequest {
        provider: ConnectProvider::Bedrock,
        endpoint: None,
        model: Some("claude-3-sonnet-20241022".to_string()),
        project: None,
        region: Some("us-gov-west-1".to_string()),
    };

    let region = req.region.as_deref().unwrap_or("us-east-1");
    let model = req.model.as_deref().unwrap_or("claude-3-sonnet-20241022");
    let cfg = ProviderConfig::bedrock(region, model);

    assert_eq!(cfg.kind, ProviderKind::Bedrock);
    assert_eq!(cfg.region.as_deref(), Some("us-gov-west-1"));
    assert!(cfg.endpoint.contains("us-gov-west-1"));
    assert!(cfg.project_id.is_none());
}

// rtmx:req REQ-TEST-044
#[test]
fn connect_bedrock_defaults_region_when_absent() {
    let req = ConnectRequest {
        provider: ConnectProvider::Bedrock,
        endpoint: None,
        model: None,
        project: None,
        region: None,
    };

    let region = req.region.as_deref().unwrap_or("us-east-1");
    let model = req.model.as_deref().unwrap_or("claude-3-sonnet-20241022");
    let cfg = ProviderConfig::bedrock(region, model);

    assert_eq!(cfg.region.as_deref(), Some("us-east-1"));
}

// ---------------------------------------------------------------------------
// Azure pipeline
// ---------------------------------------------------------------------------

// rtmx:req REQ-TEST-044
#[test]
fn connect_azure_request_maps_to_provider_config() {
    let req = ConnectRequest {
        provider: ConnectProvider::Azure,
        endpoint: Some("https://myendpoint.openai.azure.com".to_string()),
        model: Some("gpt-4o".to_string()),
        project: None,
        region: None,
    };

    let endpoint = req.endpoint.as_deref().unwrap();
    let model = req.model.as_deref().unwrap_or("gpt-4o");
    let cfg = ProviderConfig::azure(endpoint, model);

    assert_eq!(cfg.kind, ProviderKind::Azure);
    assert_eq!(cfg.endpoint, "https://myendpoint.openai.azure.com");
    assert_eq!(cfg.model, "gpt-4o");
    assert!(cfg.region.is_none());
}

// ---------------------------------------------------------------------------
// Cross-cutting: ConnectProvider <-> ProviderKind alignment
// ---------------------------------------------------------------------------

// rtmx:req REQ-TEST-044
#[test]
fn connect_provider_variants_align_with_provider_kind() {
    // Verify the mapping between TUI's ConnectProvider and aegis-llm's
    // ProviderKind stays in sync. If a new variant is added to one but
    // not the other, this test forces an update.
    let pairs: Vec<(ConnectProvider, ProviderKind)> = vec![
        (ConnectProvider::Local, ProviderKind::Local),
        (ConnectProvider::Vertex, ProviderKind::Vertex),
        (ConnectProvider::Bedrock, ProviderKind::Bedrock),
        (ConnectProvider::Azure, ProviderKind::Azure),
    ];
    for (connect, kind) in &pairs {
        let cfg = match connect {
            ConnectProvider::Local => {
                ProviderConfig::local("http://localhost:11434/v1", "llama3")
            }
            ConnectProvider::Vertex => {
                ProviderConfig::vertex("proj", "us-central1", "gemini-2.5-pro-001")
            }
            ConnectProvider::Bedrock => {
                ProviderConfig::bedrock("us-east-1", "claude-3-sonnet-20241022")
            }
            ConnectProvider::Azure => {
                ProviderConfig::azure("https://ep.openai.azure.com", "gpt-4o")
            }
        };
        assert_eq!(
            &cfg.kind, kind,
            "ConnectProvider::{connect:?} must map to ProviderKind::{kind:?}"
        );
    }
}
