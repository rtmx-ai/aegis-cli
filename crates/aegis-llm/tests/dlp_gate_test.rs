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
