//! Integration tests for the DLP transmission gate (REQ-SECURITY-018).

use aegis_llm::dlp_gate::{
    DlpGateDecision, DlpTransmissionGate, EndpointClassification, classify_endpoint,
};

// rtmx:req REQ-SECURITY-018
#[test]
fn test_dlp_gate_blocks_cui_transmission_to_commercial_endpoint() {
    let gate = DlpTransmissionGate::new();
    let content = "Engineering notes marked CUI//SP-CTI for release";
    let endpoint = "https://api.openai.com/v1/chat/completions";

    let decision = gate.check(content, endpoint);
    match decision {
        DlpGateDecision::Block { reason, matches } => {
            assert!(
                !matches.is_empty(),
                "Block decision must carry the detected matches"
            );
            assert!(
                reason.to_lowercase().contains("cui"),
                "Block reason should cite CUI: {reason}"
            );
        }
        DlpGateDecision::Allow => {
            panic!("CUI//SP-CTI must not be transmitted to a commercial endpoint");
        }
    }
}

// rtmx:req REQ-SECURITY-018
#[test]
fn test_dlp_gate_allows_cui_to_government_endpoint() {
    let gate = DlpTransmissionGate::new();
    let content = "Engineering notes marked CUI//SP-CTI";
    let endpoint = "https://example.usgovcloudapi.net/aiplatform/v1/predict";

    let decision = gate.check(content, endpoint);
    assert!(
        matches!(decision, DlpGateDecision::Allow),
        "Government endpoint must permit CUI: {decision:?}"
    );
}

// rtmx:req REQ-SECURITY-018
#[test]
fn test_dlp_gate_allows_anything_to_local() {
    let gate = DlpTransmissionGate::new();
    let content =
        "CUI//SP-CTI body with SSN 123-45-6789 and api_key: sk_live_abcdefghij1234567890";
    for endpoint in [
        "http://localhost:11434/v1/chat/completions",
        "http://127.0.0.1:8080/v1",
        "http://[::1]:11434/",
    ] {
        let decision = gate.check(content, endpoint);
        assert!(
            matches!(decision, DlpGateDecision::Allow),
            "Local endpoint {endpoint} must always allow: {decision:?}"
        );
    }
}

// rtmx:req REQ-SECURITY-018
#[test]
fn test_dlp_gate_blocks_pii_to_commercial() {
    let gate = DlpTransmissionGate::new();
    let content = "User's SSN is 123-45-6789, please confirm.";
    let endpoint = "https://api.anthropic.com/v1/messages";

    let decision = gate.check(content, endpoint);
    match decision {
        DlpGateDecision::Block { reason, matches } => {
            assert!(
                !matches.is_empty(),
                "Block decision must report at least one match"
            );
            assert!(
                reason.to_lowercase().contains("pii") || reason.to_lowercase().contains("ssn"),
                "Block reason should cite PII: {reason}"
            );
        }
        DlpGateDecision::Allow => panic!("PII must not be transmitted to commercial endpoints"),
    }
}

// rtmx:req REQ-SECURITY-018
#[test]
fn test_dlp_gate_classify_endpoints() {
    let cases: &[(&str, EndpointClassification)] = &[
        // Local / loopback
        ("http://localhost:11434/v1", EndpointClassification::Local),
        ("http://127.0.0.1:8080", EndpointClassification::Local),
        ("http://[::1]:11434/v1", EndpointClassification::Local),
        ("http://LOCALHOST:11434/v1", EndpointClassification::Local),
        // Government (GCP Vertex AI Assured Workloads)
        (
            "https://us-central1-aiplatform.googleapis.com/v1/projects/p/locations/us-central1/publishers/google/models/gemini-2.5-pro-001:streamGenerateContent",
            EndpointClassification::Government,
        ),
        // Government (AWS GovCloud)
        (
            "https://bedrock.us-gov-west-1.amazonaws.com/model/anthropic.claude-3-5-sonnet/invoke",
            EndpointClassification::Government,
        ),
        // Government (Azure Government)
        (
            "https://my-resource.openai.azure.us/openai/deployments/gpt4/chat/completions",
            EndpointClassification::Government,
        ),
        (
            "https://example.usgovcloudapi.net/openai/deployments/x",
            EndpointClassification::Government,
        ),
        // Commercial
        (
            "https://api.openai.com/v1/chat/completions",
            EndpointClassification::Commercial,
        ),
        (
            "https://api.anthropic.com/v1/messages",
            EndpointClassification::Commercial,
        ),
        (
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:streamGenerateContent",
            EndpointClassification::Commercial,
        ),
        // Unknown / default
        (
            "https://some-random-host.example.org/api",
            EndpointClassification::Unknown,
        ),
        (
            "https://us-central1-aiplatform.googleapis.com/",
            EndpointClassification::Commercial,
        ),
    ];

    for (url, expected) in cases {
        let got = classify_endpoint(url);
        assert_eq!(
            &got, expected,
            "classify_endpoint({url:?}) expected {expected:?}, got {got:?}"
        );
    }
}

// rtmx:req REQ-SECURITY-018
#[test]
fn test_dlp_gate_unknown_blocks_cui_allows_pii() {
    let gate = DlpTransmissionGate::new();
    let cui_content = "Document tagged CUI//SP-PRVCY";
    let pii_content = "Contact: user@example.com";
    let endpoint = "https://unclassified.example.com/chat";

    let cui_decision = gate.check(cui_content, endpoint);
    assert!(
        matches!(cui_decision, DlpGateDecision::Block { .. }),
        "Unknown endpoint must block CUI: {cui_decision:?}"
    );

    let pii_decision = gate.check(pii_content, endpoint);
    assert!(
        matches!(pii_decision, DlpGateDecision::Allow),
        "Unknown endpoint permits PII with warn: {pii_decision:?}"
    );
}

// rtmx:req REQ-SECURITY-018
#[test]
fn test_dlp_gate_clean_content_allowed_everywhere() {
    let gate = DlpTransmissionGate::new();
    let clean = "Please summarise the README paragraphs into three bullets.";
    for endpoint in [
        "http://localhost:11434/v1",
        "https://bedrock.us-gov-west-1.amazonaws.com/",
        "https://api.openai.com/v1/chat/completions",
        "https://random.example.com/",
    ] {
        let decision = gate.check(clean, endpoint);
        assert!(
            matches!(decision, DlpGateDecision::Allow),
            "Clean content should be allowed to {endpoint}: {decision:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// REQ-SECURITY-006: DlpGatedProvider wiring tests
//
// These test the provider-level decorator that wraps any LlmProvider and
// blocks CUI/PII before the inner provider's stream() is ever called.
// ---------------------------------------------------------------------------

mod provider_gate {
    use aegis_domain::error::DomainError;
    use aegis_domain::ports::{LlmProvider, Message, Role};
    use aegis_llm::dlp_gate::DlpGatedProvider;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn user_msg(content: &str) -> Message {
        Message::new(Role::User, content)
    }

    fn system_msg(content: &str) -> Message {
        Message::new(Role::System, content)
    }

    fn assistant_msg(content: &str) -> Message {
        Message::new(Role::Assistant, content)
    }

    /// A mock provider that tracks whether stream() was called.
    /// Returns a ProviderError so the caller can distinguish "gate
    /// allowed, inner called" from "gate blocked".
    struct TrackingProvider {
        called: AtomicBool,
    }

    impl TrackingProvider {
        fn new() -> Self {
            Self {
                called: AtomicBool::new(false),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for TrackingProvider {
        async fn stream(
            &self,
            _messages: &[Message],
            _tools: &[aegis_domain::ports::ToolSchema],
        ) -> Result<Box<dyn aegis_domain::ports::TokenStream>, DomainError> {
            self.called.store(true, Ordering::SeqCst);
            Err(DomainError::ProviderError {
                message: "mock: inner provider reached".to_string(),
            })
        }
        async fn health_check(&self) -> aegis_domain::ports::ProviderHealth {
            aegis_domain::ports::ProviderHealth::Healthy { latency_ms: 0 }
        }
    }

    // rtmx:req REQ-SECURITY-006
    #[tokio::test]
    async fn dlp_gated_provider_blocks_cui_to_commercial() {
        let provider = DlpGatedProvider::wrap(
            Box::new(TrackingProvider::new()),
            "https://api.openai.com/v1/chat/completions",
        );
        let messages = vec![user_msg("Document marked CUI//SP-CTI for review")];
        let result = provider.stream(&messages, &[]).await;
        match result {
            Err(DomainError::DlpBlocked { reason }) => {
                assert!(
                    reason.to_lowercase().contains("cui"),
                    "reason should cite CUI: {reason}"
                );
            }
            Err(e) => panic!("expected DlpBlocked, got Err: {e}"),
            Ok(_) => panic!("expected DlpBlocked, got Ok"),
        }
    }

    // rtmx:req REQ-SECURITY-006
    #[tokio::test]
    async fn dlp_gated_provider_blocks_pii_to_commercial() {
        let provider = DlpGatedProvider::wrap(
            Box::new(TrackingProvider::new()),
            "https://api.anthropic.com/v1/messages",
        );
        let messages = vec![user_msg("User SSN is 123-45-6789")];
        let result = provider.stream(&messages, &[]).await;
        assert!(
            matches!(result, Err(DomainError::DlpBlocked { .. })),
            "PII must be blocked to commercial endpoint"
        );
    }

    // rtmx:req REQ-SECURITY-006
    #[tokio::test]
    async fn dlp_gated_provider_skips_system_and_assistant_messages() {
        let provider = DlpGatedProvider::wrap(
            Box::new(TrackingProvider::new()),
            "https://api.openai.com/v1/chat/completions",
        );
        // CUI in system and assistant messages must NOT trigger the gate.
        let messages = vec![
            system_msg("System prompt with CUI//SP-CTI marking"),
            assistant_msg("Previous response mentioning CUI//SP-PRVCY"),
        ];
        let result = provider.stream(&messages, &[]).await;
        // The gate should allow non-user messages through, reaching the
        // inner provider which returns ProviderError.
        match result {
            Err(DomainError::ProviderError { message }) => {
                assert!(
                    message.contains("inner provider reached"),
                    "inner provider should have been called: {message}"
                );
            }
            Err(DomainError::DlpBlocked { reason }) => {
                panic!("gate must not block system/assistant messages: {reason}");
            }
            _ => panic!("unexpected result from mock provider"),
        }
    }

    // rtmx:req REQ-SECURITY-006
    #[tokio::test]
    async fn dlp_gated_provider_allows_clean_content_to_local() {
        let provider = DlpGatedProvider::wrap(
            Box::new(TrackingProvider::new()),
            "http://localhost:11434/v1",
        );
        // Even CUI is fine to local endpoint.
        let messages = vec![user_msg("CUI//SP-CTI is fine to localhost")];
        let result = provider.stream(&messages, &[]).await;
        match result {
            Err(DomainError::ProviderError { .. }) => {} // Inner reached, good
            Err(DomainError::DlpBlocked { reason }) => {
                panic!("local endpoint must not block: {reason}");
            }
            _ => panic!("unexpected result from mock provider"),
        }
    }

    // rtmx:req REQ-SECURITY-006
    #[tokio::test]
    async fn factory_wraps_local_provider_with_dlp_gate() {
        use aegis_llm::config::ProviderConfig;
        use aegis_llm::provider::create_provider;

        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let provider = create_provider(&cfg).expect("factory should succeed");

        // Clean content to local: DLP gate must not block.
        let messages = vec![user_msg("hello world")];
        let result = provider.stream(&messages, &[]).await;
        if let Err(DomainError::DlpBlocked { reason }) = &result {
            panic!("clean content to local must not be DLP-blocked: {reason}");
        }
        // Any other result (connection refused, etc.) is fine.
    }
}
