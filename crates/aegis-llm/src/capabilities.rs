//! Model capability detection.
//!
//! Maps known model name prefixes to their capabilities (tool_use support,
//! context window size). Unknown models receive conservative defaults.

/// Capabilities detected for a specific model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// Whether the model supports native tool/function calling.
    pub supports_tool_use: bool,
    /// Maximum context window in tokens.
    pub context_window_tokens: usize,
}

/// Known model prefix -> capabilities mapping.
///
/// Entries are checked in order; the first prefix that matches wins.
/// Keep prefixes ordered from most-specific to least-specific within
/// each model family.
const KNOWN_MODELS: &[(&str, ModelCapabilities)] = &[
    (
        "gemini-2.5",
        ModelCapabilities {
            supports_tool_use: true,
            context_window_tokens: 1_048_576,
        },
    ),
    (
        "gemini-2.0",
        ModelCapabilities {
            supports_tool_use: true,
            context_window_tokens: 1_048_576,
        },
    ),
    (
        "claude-4",
        ModelCapabilities {
            supports_tool_use: true,
            context_window_tokens: 200_000,
        },
    ),
    (
        "claude-3",
        ModelCapabilities {
            supports_tool_use: true,
            context_window_tokens: 200_000,
        },
    ),
    (
        "gpt-4o",
        ModelCapabilities {
            supports_tool_use: true,
            context_window_tokens: 128_000,
        },
    ),
    (
        "llama3",
        ModelCapabilities {
            supports_tool_use: false,
            context_window_tokens: 8_192,
        },
    ),
    (
        "granite",
        ModelCapabilities {
            supports_tool_use: false,
            context_window_tokens: 8_192,
        },
    ),
    (
        "mistral",
        ModelCapabilities {
            supports_tool_use: false,
            context_window_tokens: 32_768,
        },
    ),
];

/// Conservative defaults for unknown models: no tool_use, 4096 context.
const DEFAULT_CAPABILITIES: ModelCapabilities = ModelCapabilities {
    supports_tool_use: false,
    context_window_tokens: 4_096,
};

/// Detect capabilities for the given model name.
///
/// Matches the model name against known prefixes. The first matching
/// prefix wins. Unknown models receive conservative defaults (no
/// tool_use, 4096-token context window).
///
/// # Examples
///
/// ```
/// use aegis_llm::capabilities::detect_capabilities;
///
/// let caps = detect_capabilities("gemini-2.5-pro-001");
/// assert!(caps.supports_tool_use);
/// assert_eq!(caps.context_window_tokens, 1_048_576);
///
/// let caps = detect_capabilities("unknown-model");
/// assert!(!caps.supports_tool_use);
/// assert_eq!(caps.context_window_tokens, 4096);
/// ```
pub fn detect_capabilities(model: &str) -> ModelCapabilities {
    for (prefix, caps) in KNOWN_MODELS {
        if model.starts_with(prefix) {
            return caps.clone();
        }
    }
    DEFAULT_CAPABILITIES
}

/// Returns `true` when the ToolShim should be auto-enabled for the
/// given model. This is the case when the model does not support
/// native tool/function calling.
pub fn needs_tool_shim(model: &str) -> bool {
    !detect_capabilities(model).supports_tool_use
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Gemini family ---

    // @req REQ-LLM-017
    #[test]
    fn gemini_2_5_pro_supports_tool_use() {
        let caps = detect_capabilities("gemini-2.5-pro-001");
        assert!(caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 1_048_576);
    }

    // @req REQ-LLM-017
    #[test]
    fn gemini_2_0_flash_supports_tool_use() {
        let caps = detect_capabilities("gemini-2.0-flash-001");
        assert!(caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 1_048_576);
    }

    // --- Claude family ---

    // @req REQ-LLM-017
    #[test]
    fn claude_3_sonnet_supports_tool_use() {
        let caps = detect_capabilities("claude-3-sonnet-20241022");
        assert!(caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 200_000);
    }

    // @req REQ-LLM-017
    #[test]
    fn claude_4_supports_tool_use() {
        let caps = detect_capabilities("claude-4-opus-20260101");
        assert!(caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 200_000);
    }

    // --- GPT family ---

    // @req REQ-LLM-017
    #[test]
    fn gpt_4o_supports_tool_use() {
        let caps = detect_capabilities("gpt-4o-2024-05-13");
        assert!(caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 128_000);
    }

    // --- Local / air-gapped models ---

    // @req REQ-LLM-017
    #[test]
    fn llama3_no_tool_use() {
        let caps = detect_capabilities("llama3-8b");
        assert!(!caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 8_192);
    }

    // @req REQ-LLM-017
    #[test]
    fn granite_no_tool_use() {
        let caps = detect_capabilities("granite-3.3-2b");
        assert!(!caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 8_192);
    }

    // @req REQ-LLM-017
    #[test]
    fn mistral_no_tool_use() {
        let caps = detect_capabilities("mistral-7b-instruct");
        assert!(!caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 32_768);
    }

    // --- Unknown models get conservative defaults ---

    // @req REQ-LLM-017
    #[test]
    fn unknown_model_gets_conservative_defaults() {
        let caps = detect_capabilities("some-random-model");
        assert!(!caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 4_096);
    }

    // @req REQ-LLM-017
    #[test]
    fn empty_model_string_gets_defaults() {
        let caps = detect_capabilities("");
        assert!(!caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 4_096);
    }

    // --- Prefix matching ---

    // @req REQ-LLM-017
    #[test]
    fn prefix_match_is_case_sensitive() {
        // "Gemini" (capital G) should NOT match "gemini-2.5"
        let caps = detect_capabilities("Gemini-2.5-pro-001");
        assert!(!caps.supports_tool_use);
        assert_eq!(caps.context_window_tokens, 4_096);
    }

    // @req REQ-LLM-017
    #[test]
    fn exact_prefix_matches_without_suffix() {
        // "gemini-2.5" alone (no trailing chars) should still match
        let caps = detect_capabilities("gemini-2.5");
        assert!(caps.supports_tool_use);
    }

    // --- ToolShim auto-enable ---

    // @req REQ-LLM-017
    #[test]
    fn tool_shim_needed_for_llama3() {
        assert!(needs_tool_shim("llama3-8b"));
    }

    // @req REQ-LLM-017
    #[test]
    fn tool_shim_not_needed_for_gemini() {
        assert!(!needs_tool_shim("gemini-2.5-pro-001"));
    }

    // @req REQ-LLM-017
    #[test]
    fn tool_shim_needed_for_unknown_model() {
        assert!(needs_tool_shim("totally-unknown"));
    }
}
