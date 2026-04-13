//! aegis-agent: The agentic Read-Evaluate-Act loop.
//!
//! This crate implements the core agent loop that reads context, sends it to
//! an LLM provider (via the `LlmProvider` port), receives tool calls or text,
//! routes tool calls through the HITL gate, executes approved calls, and
//! injects results back into conversation history.

pub mod banned_commands;
pub mod cancellation;
pub mod compaction;
pub mod dedup;
pub mod dispatch;
pub mod export;
pub mod loop_runner;
pub mod mcp;
pub mod mcp_types;
pub mod rate_limiter;
pub mod repo_context;
pub mod retry;
pub mod session;
pub mod subagent;
pub mod system_prompt;
pub mod token_counter;
pub mod tools;
pub mod toolshim;
pub mod truncation;
