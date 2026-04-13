//! aegis-llm: LLM provider abstraction layer.
//!
//! Implements the `LlmProvider` port for Vertex AI, AWS Bedrock,
//! Azure OpenAI, and local OpenAI-compatible endpoints. Each provider
//! handles auth, streaming, and tool call parsing for its API.

pub mod auth;
pub mod azure;
pub mod bedrock;
pub mod bedrock_stream;
pub mod capabilities;
pub mod config;
pub mod discovery;
pub mod failover;
pub mod local;
pub mod provider;
pub mod retry;
pub mod sse;
pub mod tokens;
pub mod truncation;
pub mod validation;
pub mod vertex;
