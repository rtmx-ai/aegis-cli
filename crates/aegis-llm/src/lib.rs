//! aegis-llm: LLM provider abstraction layer.
//!
//! Implements the `LlmProvider` port for Vertex AI, AWS Bedrock, Azure OpenAI,
//! and local OpenAI-compatible endpoints. Each provider handles auth,
//! streaming, and tool call parsing for its specific API.

pub mod provider;

// Provider implementations (one module per cloud)
// pub mod vertex;    // @req REQ-LLM-001
// pub mod bedrock;   // @req REQ-LLM-002
// pub mod azure;     // @req REQ-LLM-003
// pub mod local;     // @req REQ-LLM-004
