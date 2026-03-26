//! aegis-llm: LLM provider abstraction layer.
//!
//! Implements the `LlmProvider` port for Vertex AI, AWS Bedrock,
//! Azure OpenAI, and local OpenAI-compatible endpoints. Each provider
//! handles auth, streaming, and tool call parsing for its API.

pub mod capabilities;
pub mod config;
pub mod local;
pub mod provider;
pub mod retry;
pub mod tokens;
pub mod validation;
