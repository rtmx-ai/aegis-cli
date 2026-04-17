//! Integration tests for adversary review wired into the agent loop.
//!
//! rtmx:req REQ-SECURITY-004

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aegis_agent::adversary_bridge::{
    AdversaryReviewer, ReviewAssessment, ReviewDecision, ReviewMode, RiskLevel,
};
use aegis_agent::loop_runner::{AgentConfig, AgentLoop};
use aegis_domain::ports::*;
use aegis_domain::types::*;
use async_trait::async_trait;

use aegis_test_support::mock_executor::MockToolExecutor;
use aegis_test_support::mock_filter::MockSecurityFilter;
use aegis_test_support::mock_gate::MockApprovalGate;
use aegis_test_support::mock_ledger::MockAuditLedger;
use aegis_test_support::mock_provider::MockLlmProvider;

// ---------------------------------------------------------------------------
// Mock adversary reviewer
// ---------------------------------------------------------------------------

/// A mock adversary reviewer that always returns a fixed risk level.
struct MockAdversaryReviewer {
    risk: RiskLevel,
    /// Number of times `review()` was actually called (not short-circuited
    /// by Off mode -- that is handled by the agent loop, not this mock).
    call_count: AtomicUsize,
}

impl MockAdversaryReviewer {
    fn new(risk: RiskLevel) -> Self {
        Self {
            risk,
            call_count: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AdversaryReviewer for MockAdversaryReviewer {
    async fn review(
        &self,
        _tool_call: &ToolCall,
        _context: &[Message],
        mode: ReviewMode,
    ) -> Result<ReviewDecision, String> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if matches!(mode, ReviewMode::Off) {
            return Ok(ReviewDecision::Allow { assessment: None });
        }

        let assessment = ReviewAssessment {
            risk: self.risk,
            reasoning: format!("Mock assessment: {:?}", self.risk),
            indicators: vec!["mock".to_string()],
        };

        match mode {
            ReviewMode::Off => unreachable!(),
            ReviewMode::Warn => Ok(ReviewDecision::Allow {
                assessment: Some(assessment),
            }),
            ReviewMode::Enforce { threshold } => {
                if self.risk >= threshold {
                    Ok(ReviewDecision::Block { assessment })
                } else {
                    Ok(ReviewDecision::Allow {
                        assessment: Some(assessment),
                    })
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper to build agents with adversary
// ---------------------------------------------------------------------------

fn make_agent_with_adversary(
    provider: MockLlmProvider,
    gate: MockApprovalGate,
    reviewer: Arc<dyn AdversaryReviewer>,
    mode: ReviewMode,
) -> AgentLoop<
    MockLlmProvider,
    MockApprovalGate,
    MockToolExecutor,
    MockAuditLedger,
    MockSecurityFilter,
> {
    AgentLoop::new(
        provider,
        gate,
        MockToolExecutor::new(),
        MockAuditLedger::new(),
        MockSecurityFilter,
        AgentConfig::default(),
    )
    .with_adversary(reviewer, mode)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// rtmx:req REQ-SECURITY-004
#[tokio::test]
async fn test_adversary_blocks_dangerous_tool_call() {
    let provider = MockLlmProvider::new();

    // LLM requests a write (state-mutating).
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "rm -rf /".to_string(),
        }),
        StreamEvent::Done {
            input_tokens: 10,
            output_tokens: 5,
        },
    ]);

    // LLM sees the blocked result and responds.
    provider.queue_response(vec![
        StreamEvent::Token("Blocked by adversary.".to_string()),
        StreamEvent::Done {
            input_tokens: 20,
            output_tokens: 5,
        },
    ]);

    let reviewer = Arc::new(MockAdversaryReviewer::new(RiskLevel::Critical));

    let agent = make_agent_with_adversary(
        provider,
        MockApprovalGate::always_approve(),
        reviewer.clone(),
        ReviewMode::Enforce {
            threshold: RiskLevel::High,
        },
    );

    let result = agent.run("Do something dangerous").await.unwrap();

    // The tool call should have been blocked.
    assert_eq!(result.response, "Blocked by adversary.");
    // Adversary was consulted.
    assert_eq!(reviewer.call_count(), 1);
}

// rtmx:req REQ-SECURITY-004
#[tokio::test]
async fn test_adversary_warn_mode_logs_but_allows() {
    let provider = MockLlmProvider::new();

    // LLM requests a write.
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/lib.rs"),
            content: "code".to_string(),
        }),
        StreamEvent::Done {
            input_tokens: 10,
            output_tokens: 5,
        },
    ]);

    // LLM gives final answer.
    provider.queue_response(vec![
        StreamEvent::Token("Done writing.".to_string()),
        StreamEvent::Done {
            input_tokens: 20,
            output_tokens: 3,
        },
    ]);

    let reviewer = Arc::new(MockAdversaryReviewer::new(RiskLevel::Critical));

    let agent = make_agent_with_adversary(
        provider,
        MockApprovalGate::always_approve(),
        reviewer.clone(),
        ReviewMode::Warn,
    );

    let result = agent.run("Write some code").await.unwrap();

    // In Warn mode, tool call proceeds despite Critical risk.
    assert_eq!(result.response, "Done writing.");
    // Adversary was still consulted.
    assert_eq!(reviewer.call_count(), 1);
}

// rtmx:req REQ-SECURITY-004
#[tokio::test]
async fn test_adversary_off_mode_skips_review() {
    let provider = MockLlmProvider::new();

    // LLM requests a write.
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/lib.rs"),
            content: "code".to_string(),
        }),
        StreamEvent::Done {
            input_tokens: 10,
            output_tokens: 5,
        },
    ]);

    // LLM gives final answer.
    provider.queue_response(vec![
        StreamEvent::Token("Wrote it.".to_string()),
        StreamEvent::Done {
            input_tokens: 20,
            output_tokens: 2,
        },
    ]);

    let reviewer = Arc::new(MockAdversaryReviewer::new(RiskLevel::Critical));

    let agent = make_agent_with_adversary(
        provider,
        MockApprovalGate::always_approve(),
        reviewer.clone(),
        ReviewMode::Off,
    );

    let result = agent.run("Write code").await.unwrap();
    assert_eq!(result.response, "Wrote it.");
    // Adversary should NOT have been called -- Off mode is
    // short-circuited in the agent loop before calling review().
    assert_eq!(reviewer.call_count(), 0);
}

// rtmx:req REQ-SECURITY-004
#[tokio::test]
async fn test_adversary_integrates_with_hitl_gate() {
    // Adversary allows (Medium risk, threshold=High), then HITL gate
    // triggers for the mutating write_file tool. Verify both gates hit.
    let provider = MockLlmProvider::new();

    // LLM requests a write (state-mutating -> HITL gate).
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/foo.rs"),
            content: "safe code".to_string(),
        }),
        StreamEvent::Done {
            input_tokens: 10,
            output_tokens: 5,
        },
    ]);

    // LLM gives final answer.
    provider.queue_response(vec![
        StreamEvent::Token("Written.".to_string()),
        StreamEvent::Done {
            input_tokens: 20,
            output_tokens: 2,
        },
    ]);

    let reviewer = Arc::new(MockAdversaryReviewer::new(RiskLevel::Medium));

    // Use always_deny gate -- if the adversary allows but HITL denies,
    // we should see PermissionDenied injected.
    let agent = make_agent_with_adversary(
        provider,
        MockApprovalGate::always_deny(),
        reviewer.clone(),
        ReviewMode::Enforce {
            threshold: RiskLevel::High,
        },
    );

    let result = agent.run("Write safe code").await.unwrap();

    // Adversary allowed (Medium < High), but HITL denied.
    assert_eq!(result.response, "Written.");
    // Adversary was consulted.
    assert_eq!(reviewer.call_count(), 1);
}

// rtmx:req REQ-SECURITY-004
#[tokio::test]
async fn test_adversary_enforce_below_threshold_allows() {
    let provider = MockLlmProvider::new();

    // LLM requests a read (safe, auto-executes, but adversary still reviews).
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::ReadFile {
            path: FilePath::new_unchecked("readme.md"),
        }),
        StreamEvent::Done {
            input_tokens: 10,
            output_tokens: 5,
        },
    ]);

    provider.queue_response(vec![
        StreamEvent::Token("Read it.".to_string()),
        StreamEvent::Done {
            input_tokens: 20,
            output_tokens: 2,
        },
    ]);

    let reviewer = Arc::new(MockAdversaryReviewer::new(RiskLevel::Low));

    let agent = make_agent_with_adversary(
        provider,
        MockApprovalGate::always_deny(),
        reviewer.clone(),
        ReviewMode::Enforce {
            threshold: RiskLevel::High,
        },
    );

    let result = agent.run("Read readme").await.unwrap();

    // Low risk < High threshold, so adversary allows. read_file is safe
    // so HITL is not consulted either.
    assert_eq!(result.response, "Read it.");
    assert_eq!(reviewer.call_count(), 1);
}
