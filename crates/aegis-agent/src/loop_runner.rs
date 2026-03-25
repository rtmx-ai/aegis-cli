//! The REA (Read-Evaluate-Act) loop runner.

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;

/// Configuration for the agent loop.
pub struct AgentConfig {
    pub max_iterations: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
        }
    }
}

/// The agent loop runner, parameterized by port traits.
#[allow(dead_code)]
pub struct AgentLoop<P, G, E, A, S>
where
    P: LlmProvider,
    G: ApprovalGate,
    E: ToolExecutor,
    A: AuditLedger,
    S: SecurityFilter,
{
    provider: P,
    gate: G,
    executor: E,
    ledger: A,
    filter: S,
    config: AgentConfig,
}

impl<P, G, E, A, S> AgentLoop<P, G, E, A, S>
where
    P: LlmProvider,
    G: ApprovalGate,
    E: ToolExecutor,
    A: AuditLedger,
    S: SecurityFilter,
{
    pub fn new(
        provider: P,
        gate: G,
        executor: E,
        ledger: A,
        filter: S,
        config: AgentConfig,
    ) -> Self {
        Self {
            provider,
            gate,
            executor,
            ledger,
            filter,
            config,
        }
    }

    /// Run the agent loop to completion for a given user prompt.
    pub async fn run(&self, _prompt: &str) -> Result<String, DomainError> {
        // TODO: Implement REA loop
        // 1. Read: Assemble context (prompt + history + tool schemas)
        // 2. Evaluate: Stream to LLM provider
        // 3. Act: Route tool calls through HITL gate, execute, inject results
        // 4. Loop until resolved or max_iterations
        todo!("REA loop implementation")
    }
}

#[cfg(test)]
mod tests {
    // @req REQ-AGENT-001
    #[test]
    fn test_agent_config_defaults() {
        let config = super::AgentConfig::default();
        assert_eq!(config.max_iterations, 100);
    }
}
