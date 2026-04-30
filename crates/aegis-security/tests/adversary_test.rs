//! Integration tests for the adversary review chain.
//!
//! Covers REQ-SECURITY-011 (spawn + risk classification),
//! REQ-SECURITY-012 (enforcement modes off/warn/enforce),
//! and REQ-SECURITY-013 (audit trail for assessments).

use std::sync::Arc;
use std::sync::Mutex;

use aegis_domain::ports::{LlmProvider, Message, Role, StreamEvent};
use aegis_domain::types::{FilePath, ToolCall};
use aegis_security::adversary::{
    AdversaryAgent, AdversaryAuditSink, AdversaryError, EnforcementDecision, EnforcementMode,
    RiskLevel,
};
use aegis_test_support::mock_provider::MockLlmProvider;
use async_trait::async_trait;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Queue a canned provider response that produces a single Token event with
/// the given body text, followed by a terminal Done event.
fn queue_text(provider: &MockLlmProvider, body: &str) {
    provider.queue_response(vec![
        StreamEvent::Token(body.to_string()),
        StreamEvent::Done {
            input_tokens: 0,
            output_tokens: 0,
        },
    ]);
}

fn sample_read_call() -> ToolCall {
    ToolCall::ReadFile {
        path: FilePath::new_unchecked("src/main.rs"),
    }
}

fn sample_destructive_call() -> ToolCall {
    ToolCall::RunCommand {
        command: "rm -rf /".into(),
        timeout_secs: 5,
    }
}

fn user_context() -> Vec<Message> {
    vec![Message {
        role: Role::User,
        content: "please review".into(),
        cache_control: None,
    }]
}

/// In-memory audit sink for test assertions.
#[derive(Default)]
struct RecordingSink {
    entries: Mutex<Vec<(EnforcementDecision, EnforcementMode)>>,
}

#[async_trait]
impl AdversaryAuditSink for RecordingSink {
    async fn record_assessment(&self, decision: &EnforcementDecision, mode: EnforcementMode) {
        self.entries.lock().unwrap().push((decision.clone(), mode));
    }
}

impl RecordingSink {
    fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
    fn last(&self) -> Option<(EnforcementDecision, EnforcementMode)> {
        self.entries.lock().unwrap().last().cloned()
    }
}

// ---------------------------------------------------------------------------
// REQ-SECURITY-011 tests
// ---------------------------------------------------------------------------

// rtmx:req REQ-SECURITY-011
#[tokio::test]
async fn test_adversary_classifies_risk_level() {
    // Table-driven: each risk level is parsed back from the mock response.
    let cases = [
        ("Low", RiskLevel::Low),
        ("Medium", RiskLevel::Medium),
        ("High", RiskLevel::High),
        ("Critical", RiskLevel::Critical),
    ];

    for (label, expected) in cases {
        let provider = Arc::new(MockLlmProvider::new());
        queue_text(
            &provider,
            &format!(
                "RISK: {label}\n\
                 REASONING: operation read-only on tracked source\n\
                 INDICATORS: read_file, project-path"
            ),
        );
        let adversary = AdversaryAgent::new(provider.clone());
        let assessment = adversary
            .classify(&sample_read_call(), &user_context())
            .await
            .expect("classify should parse");
        assert_eq!(assessment.risk, expected, "label {label}");
        assert!(!assessment.reasoning.is_empty());
        assert!(!assessment.indicators.is_empty());
    }
}

// rtmx:req REQ-SECURITY-011
#[tokio::test]
async fn test_adversary_critical_for_destructive_command() {
    let provider = Arc::new(MockLlmProvider::new());
    queue_text(
        &provider,
        "RISK: Critical\n\
         REASONING: recursive delete of root filesystem is irreversible\n\
         INDICATORS: rm -rf, root path, destructive",
    );
    let adversary = AdversaryAgent::new(provider);
    let assessment = adversary
        .classify(&sample_destructive_call(), &user_context())
        .await
        .expect("classify should parse");
    assert_eq!(assessment.risk, RiskLevel::Critical);
    assert!(
        assessment
            .indicators
            .iter()
            .any(|i| i.to_lowercase().contains("rm"))
    );
}

// rtmx:req REQ-SECURITY-011
#[tokio::test]
async fn test_adversary_low_for_read_file() {
    let provider = Arc::new(MockLlmProvider::new());
    queue_text(
        &provider,
        "RISK: Low\n\
         REASONING: read-only access to project file\n\
         INDICATORS: read_file",
    );
    let adversary = AdversaryAgent::new(provider);
    let assessment = adversary
        .classify(&sample_read_call(), &user_context())
        .await
        .expect("classify should parse");
    assert_eq!(assessment.risk, RiskLevel::Low);
}

// rtmx:req REQ-SECURITY-011
#[tokio::test]
async fn test_adversary_handles_malformed_response() {
    let provider = Arc::new(MockLlmProvider::new());
    queue_text(&provider, "this is not the structured format at all");
    let adversary = AdversaryAgent::new(provider);
    let err = adversary
        .classify(&sample_read_call(), &user_context())
        .await
        .expect_err("malformed response must error");
    assert!(matches!(err, AdversaryError::ParseError(_)));
}

// rtmx:req REQ-SECURITY-011
#[tokio::test]
async fn test_adversary_system_prompt_sent_to_provider() {
    // The adversary should send a system prompt establishing its reviewer
    // role as the first message to the provider. We capture messages via
    // a custom spy provider.
    struct SpyProvider {
        inner: Arc<MockLlmProvider>,
        captured: Mutex<Vec<Message>>,
    }
    #[async_trait]
    impl LlmProvider for SpyProvider {
        async fn stream(
            &self,
            messages: &[aegis_domain::ports::Message],
            tools: &[aegis_domain::ports::ToolSchema],
        ) -> Result<Box<dyn aegis_domain::ports::TokenStream>, aegis_domain::error::DomainError>
        {
            *self.captured.lock().unwrap() = messages.to_vec();
            self.inner.stream(messages, tools).await
        }
    }

    let inner = Arc::new(MockLlmProvider::new());
    queue_text(&inner, "RISK: Low\nREASONING: ok\nINDICATORS: read_file");
    let spy = Arc::new(SpyProvider {
        inner: inner.clone(),
        captured: Mutex::new(Vec::new()),
    });
    let adversary = AdversaryAgent::new(spy.clone());
    let _ = adversary
        .classify(&sample_read_call(), &user_context())
        .await
        .unwrap();

    let captured = spy.captured.lock().unwrap().clone();
    assert!(!captured.is_empty(), "provider must receive messages");
    assert_eq!(
        captured[0].role,
        Role::System,
        "first message must be the adversary system prompt"
    );
    let lower = captured[0].content.to_lowercase();
    assert!(
        lower.contains("security") && lower.contains("review"),
        "system prompt should declare security review role: {}",
        captured[0].content
    );
    assert!(
        lower.contains("low")
            && lower.contains("medium")
            && lower.contains("high")
            && lower.contains("critical"),
        "system prompt should enumerate all four risk levels"
    );
}

// ---------------------------------------------------------------------------
// REQ-SECURITY-012 tests
// ---------------------------------------------------------------------------

// rtmx:req REQ-SECURITY-012
#[tokio::test]
async fn test_adversary_enforcement_modes() {
    // Table-driven: (mode, mocked_risk_label, expected_allow_vs_block)
    let cases = [
        (EnforcementMode::Off, "Critical", true),
        (EnforcementMode::Warn, "Critical", true),
        (
            EnforcementMode::Enforce {
                threshold: RiskLevel::High,
            },
            "Critical",
            false,
        ),
        (
            EnforcementMode::Enforce {
                threshold: RiskLevel::High,
            },
            "Medium",
            true,
        ),
    ];

    for (mode, label, expect_allow) in cases {
        let provider = Arc::new(MockLlmProvider::new());
        // Off mode won't call the provider, but queue a response anyway
        // so that if it were called, parsing would succeed.
        queue_text(
            &provider,
            &format!("RISK: {label}\nREASONING: r\nINDICATORS: x"),
        );
        let adversary = AdversaryAgent::new(provider);
        let decision = adversary
            .evaluate(&sample_read_call(), &user_context(), mode)
            .await
            .expect("evaluate should not error");
        match (expect_allow, &decision) {
            (true, EnforcementDecision::Allow { .. }) => {}
            (false, EnforcementDecision::Block { .. }) => {}
            (_, got) => panic!("mode {mode:?} label {label}: got {got:?}"),
        }
    }
}

// rtmx:req REQ-SECURITY-012
#[tokio::test]
async fn test_enforce_blocks_at_or_above_threshold() {
    // threshold = High; Critical must block, High must block,
    // Medium must allow.
    let scenarios = [
        ("Critical", false),
        ("High", false),
        ("Medium", true),
        ("Low", true),
    ];
    for (label, expect_allow) in scenarios {
        let provider = Arc::new(MockLlmProvider::new());
        queue_text(
            &provider,
            &format!("RISK: {label}\nREASONING: r\nINDICATORS: x"),
        );
        let adversary = AdversaryAgent::new(provider);
        let decision = adversary
            .evaluate(
                &sample_read_call(),
                &user_context(),
                EnforcementMode::Enforce {
                    threshold: RiskLevel::High,
                },
            )
            .await
            .unwrap();
        match (expect_allow, &decision) {
            (
                true,
                EnforcementDecision::Allow {
                    assessment: Some(_),
                },
            ) => {}
            (false, EnforcementDecision::Block { .. }) => {}
            (_, got) => panic!("label {label}: got {got:?}"),
        }
    }
}

// rtmx:req REQ-SECURITY-012
#[tokio::test]
async fn test_warn_never_blocks() {
    // Even Critical risk under Warn returns Allow, with assessment present.
    let provider = Arc::new(MockLlmProvider::new());
    queue_text(
        &provider,
        "RISK: Critical\nREASONING: destructive\nINDICATORS: rm, root",
    );
    let adversary = AdversaryAgent::new(provider);
    let decision = adversary
        .evaluate(
            &sample_destructive_call(),
            &user_context(),
            EnforcementMode::Warn,
        )
        .await
        .unwrap();
    match decision {
        EnforcementDecision::Allow {
            assessment: Some(a),
        } => {
            assert_eq!(a.risk, RiskLevel::Critical);
        }
        other => panic!("warn must never block; got {other:?}"),
    }
}

// rtmx:req REQ-SECURITY-012
#[tokio::test]
async fn test_off_skips_assessment() {
    // Off mode returns Allow{None} and never calls the provider.
    let provider = Arc::new(MockLlmProvider::new());
    // Do NOT queue any response. If the adversary calls the provider,
    // the mock will return a ProviderError and the test will fail.
    let adversary = AdversaryAgent::new(provider.clone());
    let decision = adversary
        .evaluate(&sample_read_call(), &user_context(), EnforcementMode::Off)
        .await
        .expect("Off mode must not error");
    assert!(
        matches!(decision, EnforcementDecision::Allow { assessment: None }),
        "Off returns Allow with no assessment, got {decision:?}"
    );
    // Mock never called: captured schemas stays empty.
    assert!(
        provider.captured_tool_schemas.lock().unwrap().is_empty(),
        "provider must not be called in Off mode"
    );
}

// ---------------------------------------------------------------------------
// REQ-SECURITY-013 tests
// ---------------------------------------------------------------------------

// rtmx:req REQ-SECURITY-013
#[tokio::test]
async fn test_adversary_writes_assessments_to_audit_trail() {
    let provider = Arc::new(MockLlmProvider::new());
    queue_text(
        &provider,
        "RISK: High\nREASONING: writes system location\nINDICATORS: write_file, etc",
    );
    let adversary = AdversaryAgent::new(provider);
    let sink = RecordingSink::default();

    let decision = adversary
        .evaluate_and_record(
            &sample_read_call(),
            &user_context(),
            EnforcementMode::Enforce {
                threshold: RiskLevel::Critical,
            },
            &sink,
        )
        .await
        .unwrap();

    assert_eq!(sink.len(), 1, "sink must record exactly one entry");
    let (recorded, mode) = sink.last().unwrap();
    assert_eq!(
        mode,
        EnforcementMode::Enforce {
            threshold: RiskLevel::Critical,
        }
    );
    // Returned decision should equal the one recorded.
    assert_eq!(recorded, decision);
    match recorded {
        EnforcementDecision::Allow {
            assessment: Some(a),
        } => {
            assert_eq!(a.risk, RiskLevel::High);
            assert!(!a.reasoning.is_empty());
            assert!(!a.indicators.is_empty());
        }
        other => panic!("expected Allow with assessment, got {other:?}"),
    }
}

// rtmx:req REQ-SECURITY-013
#[tokio::test]
async fn test_audit_trail_records_block_decisions() {
    let provider = Arc::new(MockLlmProvider::new());
    queue_text(
        &provider,
        "RISK: Critical\nREASONING: destructive\nINDICATORS: rm",
    );
    let adversary = AdversaryAgent::new(provider);
    let sink = RecordingSink::default();

    let decision = adversary
        .evaluate_and_record(
            &sample_destructive_call(),
            &user_context(),
            EnforcementMode::Enforce {
                threshold: RiskLevel::High,
            },
            &sink,
        )
        .await
        .unwrap();

    assert!(matches!(decision, EnforcementDecision::Block { .. }));
    assert_eq!(sink.len(), 1);
    let (recorded, _mode) = sink.last().unwrap();
    match recorded {
        EnforcementDecision::Block { assessment } => {
            assert_eq!(assessment.risk, RiskLevel::Critical);
        }
        other => panic!("expected Block, got {other:?}"),
    }
}

// rtmx:req REQ-SECURITY-013
#[tokio::test]
async fn test_audit_trail_records_allow_decisions_under_warn() {
    let provider = Arc::new(MockLlmProvider::new());
    queue_text(
        &provider,
        "RISK: Critical\nREASONING: destructive\nINDICATORS: rm",
    );
    let adversary = AdversaryAgent::new(provider);
    let sink = RecordingSink::default();

    let decision = adversary
        .evaluate_and_record(
            &sample_destructive_call(),
            &user_context(),
            EnforcementMode::Warn,
            &sink,
        )
        .await
        .unwrap();

    assert!(matches!(
        decision,
        EnforcementDecision::Allow {
            assessment: Some(_)
        }
    ));
    assert_eq!(sink.len(), 1);
    let (recorded, mode) = sink.last().unwrap();
    assert_eq!(mode, EnforcementMode::Warn);
    assert!(matches!(
        recorded,
        EnforcementDecision::Allow {
            assessment: Some(_)
        }
    ));
}

// rtmx:req REQ-SECURITY-013
#[tokio::test]
async fn test_off_mode_records_nothing() {
    // In Off mode the adversary does not classify, so there is nothing
    // meaningful to record. The sink must remain empty.
    let provider = Arc::new(MockLlmProvider::new());
    // Intentionally no queued response -- Off should never call the provider.
    let adversary = AdversaryAgent::new(provider);
    let sink = RecordingSink::default();

    let decision = adversary
        .evaluate_and_record(
            &sample_read_call(),
            &user_context(),
            EnforcementMode::Off,
            &sink,
        )
        .await
        .unwrap();

    assert!(matches!(
        decision,
        EnforcementDecision::Allow { assessment: None }
    ));
    assert_eq!(sink.len(), 0, "Off mode must not record to the audit sink");
}
