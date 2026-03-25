//! Provider factory and registry.

// TODO: Implement provider factory pattern (Goose-style).
// Each provider implements aegis_domain::ports::LlmProvider.
// Factory selects based on config: vertex | bedrock | azure | local.
