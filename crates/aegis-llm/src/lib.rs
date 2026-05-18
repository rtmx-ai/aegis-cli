//! aegis-llm: LLM provider abstraction layer.
//!
//! Implements the `LlmProvider` port for Vertex AI, AWS Bedrock,
//! Azure OpenAI, and local OpenAI-compatible endpoints. Each provider
//! handles auth, streaming, and tool call parsing for its API.

pub mod auth;
pub mod auth_manager;
pub mod azure;
pub mod bedrock;
pub mod bedrock_stream;
pub mod capabilities;
pub mod config;
pub mod csp_discovery;
pub mod discovery;
pub mod dlp_gate;
pub mod energy;
pub mod failover;
pub mod local;
pub mod model_origin;
pub mod provider;
pub mod providers;
pub mod rates;
pub mod retry;
pub mod sse;
pub mod tokens;
pub mod truncation;
pub mod validation;
pub mod vertex;
