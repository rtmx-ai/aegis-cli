//! aegis-agent: The agentic Read-Evaluate-Act loop.
//!
//! This crate implements the core agent loop that reads context, sends it to
//! an LLM provider (via the `LlmProvider` port), receives tool calls or text,
//! routes tool calls through the HITL gate, executes approved calls, and
//! injects results back into conversation history.

pub mod loop_runner;
pub mod tools;
