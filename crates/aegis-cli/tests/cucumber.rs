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
            .finish()
    }
}

#[tokio::main]
async fn main() {
    AegisWorld::cucumber()
        .filter_run("../../tests/features", |_, _, sc| {
            // Skip scenarios tagged @wip until their step definitions exist.
            !sc.tags.iter().any(|t| t == "wip")
        })
        .await;
}

// Sanity tests for the harness itself. Cucumber's main() is the
// production runner; these tests verify the AegisWorld foundation
// independently and run alongside the cucumber suite when invoked
// via `cargo test --test cucumber`.

/// @req REQ-TEST-020
/// Sanity check that the AegisWorld constructs cleanly.
#[test]
fn test_aegis_world_default_constructs() {
    let world = AegisWorld::default();
    assert!(world.provider.is_none());
    assert!(world.user_prompt.is_none());
    assert!(world.tool_calls_seen.is_empty());
    assert!(world.final_response.is_none());
    assert!(world.last_error.is_none());
}

/// @req REQ-TEST-020
/// Verify the World can be populated and read back.
#[test]
fn test_aegis_world_state_round_trip() {
    let mut world = AegisWorld::default();
    world.user_prompt = Some("hello".into());
    world.final_response = Some("hi there".into());
    assert_eq!(world.user_prompt.as_deref(), Some("hello"));
    assert_eq!(world.final_response.as_deref(), Some("hi there"));
}
