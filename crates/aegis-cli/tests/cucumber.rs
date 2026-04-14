//! REQ-TEST-020: Cucumber test runner with AegisWorld foundation.
//!
//! This is the entry point for running BDD scenarios against the real
//! aegis-cli ports (using mocks from aegis-test-support where the test
//! needs to pin LLM responses or audit assertions).
//!
//! Run: cargo test --test cucumber --package aegis-cli
//! Run a single scenario: cargo test --test cucumber -- --tags "@req REQ-AGENT-001"
//!
//! Feature files live at the workspace root in tests/features/.
//! Each scenario gets a fresh AegisWorld via Default::default().

use cucumber::World;

mod steps;

/// Test-scoped state for a single Cucumber scenario.
///
/// Holds the mocks and intermediate state needed to drive aegis-cli's
/// agent loop, security filter, and HITL gate. Each scenario constructs
/// a fresh `AegisWorld` so tests are fully isolated.
#[derive(Default, World)]
pub struct AegisWorld {
    /// Mock LLM provider with pre-queued stream events.
    pub provider: Option<aegis_test_support::mock_provider::MockLlmProvider>,
    /// Last user prompt sent to the agent.
    pub user_prompt: Option<String>,
    /// Recorded tool calls the agent attempted.
    pub tool_calls_seen: Vec<aegis_domain::types::ToolCall>,
    /// Final assistant response text (after the loop completes).
    pub final_response: Option<String>,
    /// Last error message produced by the system under test.
    pub last_error: Option<String>,

    // -- Security (REQ-SECURITY-001) --
    /// Security filter for .aegisignore tests.
    pub security_filter: Option<aegis_security::aegisignore::AegisIgnore>,
    /// Temporary workspace directory for file-system-backed scenarios.
    pub temp_dir: Option<tempfile::TempDir>,
    /// Result of the last tool invocation (for assertion steps).
    pub tool_result: Option<Result<String, String>>,

    // -- HITL (REQ-HITL-001, REQ-HITL-002) --
    /// Pending tool call for HITL scenarios.
    pub pending_tool_call: Option<aegis_domain::types::ToolCall>,
    /// HITL approval decision from the simulated user.
    pub approval_decision: Option<aegis_domain::types::ApprovalDecision>,

    // -- Audit (REQ-AUDIT-001) --
    /// In-memory audit ledger for assertion.
    pub audit_ledger: Option<aegis_test_support::mock_ledger::MockAuditLedger>,
    /// Session ID for audit scenarios.
    pub session_id: Option<aegis_domain::types::SessionId>,
}

// Manual Debug impl since MockLlmProvider doesn't implement Debug.
impl std::fmt::Debug for AegisWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AegisWorld")
            .field(
                "provider",
                &self.provider.as_ref().map(|_| "<MockLlmProvider>"),
            )
            .field("user_prompt", &self.user_prompt)
            .field("tool_calls_seen", &self.tool_calls_seen)
            .field("final_response", &self.final_response)
            .field("last_error", &self.last_error)
            .field("security_filter", &self.security_filter.is_some())
            .field("temp_dir", &self.temp_dir.is_some())
            .field("tool_result", &self.tool_result)
            .field("pending_tool_call", &self.pending_tool_call)
            .field("approval_decision", &self.approval_decision)
            .field("audit_ledger", &self.audit_ledger.is_some())
            .field("session_id", &self.session_id)
            .finish()
    }
}

#[tokio::main]
async fn main() {
    AegisWorld::cucumber()
        .fail_on_skipped()
        .filter_run("../../tests/features", |feat, _, sc| {
            // Skip scenarios tagged @wip until their step definitions exist.
            // Check both feature-level and scenario-level tags.
            let mut dominated = feat.tags.iter().chain(sc.tags.iter());
            !dominated.any(|t| t == "wip")
        })
        .await;
}

// NOTE: AegisWorld sanity tests have been moved to cucumber_world_test.rs
// which uses the standard test harness (harness = true). The #[test]
// functions that were here never ran because this binary uses a custom
// main() with harness = false. See REQ-TEST-020.
