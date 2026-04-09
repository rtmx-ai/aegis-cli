//! aegis-test-support: Shared test infrastructure.
//!
//! Provides mock implementations of domain ports, recording/replay helpers
//! for deterministic LLM testing, and common test fixtures.

pub mod fixtures;
pub mod mock_executor;
pub mod mock_filter;
pub mod mock_gate;
pub mod mock_ledger;
pub mod mock_provider;
pub mod recorder;
pub mod wiremock_llm;
