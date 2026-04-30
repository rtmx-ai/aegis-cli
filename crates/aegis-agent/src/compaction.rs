//! Context window compaction for conversation history.
//!
//! When the conversation approaches the context window token limit, older
//! user/assistant/tool message pairs are replaced with a single summary
//! placeholder. System messages are never dropped, and the most recent N
//! messages are always preserved.

use aegis_domain::ports::{Message, Role};

use crate::token_counter::estimate_messages;

/// Placeholder content inserted in place of compacted messages.
const COMPACTION_PLACEHOLDER: &str = "[Earlier conversation compacted]";

/// Configuration for context window compaction.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Total context window size in tokens.
    pub context_window: usize,
    /// Fraction of context window that triggers compaction (0.0..=1.0).
    pub threshold_ratio: f64,
    /// Number of most-recent messages to always keep.
    pub keep_recent: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            context_window: 128_000,
            threshold_ratio: 0.85,
            keep_recent: 10,
        }
    }
}

/// Result of a compaction operation.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// The compacted message list.
    pub messages: Vec<Message>,
    /// Number of tokens freed by compaction.
    pub tokens_freed: usize,
    /// Number of messages that were dropped.
    pub messages_dropped: usize,
}

/// Returns true if the estimated token count exceeds the compaction threshold.
pub fn needs_compaction(messages: &[Message], config: &CompactionConfig) -> bool {
    let total = estimate_messages(messages);
    let threshold = (config.context_window as f64 * config.threshold_ratio) as usize;
    total > threshold
}

/// Compact conversation history by replacing older non-system messages with a
/// single summary placeholder.
///
/// System messages are always preserved. The most recent `keep_recent` messages
/// are always preserved. All other messages are dropped and replaced with one
/// placeholder message.
pub fn compact(messages: &[Message], config: &CompactionConfig) -> CompactionResult {
    let tokens_before = estimate_messages(messages);

    // If the conversation is short enough, nothing to compact.
    if messages.len() <= config.keep_recent {
        return CompactionResult {
            messages: messages.to_vec(),
            tokens_freed: 0,
            messages_dropped: 0,
        };
    }

    // Split into candidate region and preserved tail.
    let split_point = messages.len() - config.keep_recent;
    let candidate_region = &messages[..split_point];
    let preserved_tail = &messages[split_point..];

    // From the candidate region, always keep system messages.
    let mut result: Vec<Message> = candidate_region
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .cloned()
        .collect();

    let messages_dropped = candidate_region.len() - result.len();

    // Only insert the placeholder if we actually dropped messages.
    if messages_dropped > 0 {
        result.push(Message {
            role: Role::System,
            content: COMPACTION_PLACEHOLDER.to_string(),
            cache_control: None,
        });
    }

    // Append preserved recent messages.
    result.extend_from_slice(preserved_tail);

    let tokens_after = estimate_messages(&result);
    let tokens_freed = tokens_before.saturating_sub(tokens_after);

    CompactionResult {
        messages: result,
        tokens_freed,
        messages_dropped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            cache_control: None,
        }
    }

    fn default_config() -> CompactionConfig {
        CompactionConfig {
            context_window: 100,
            threshold_ratio: 0.85,
            keep_recent: 3,
        }
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn needs_compaction_returns_false_below_threshold() {
        let config = default_config(); // threshold = 85 tokens
        // Single short message: ~5 tokens (1 content + 4 overhead)
        let messages = vec![msg(Role::User, "hi")];
        assert!(!needs_compaction(&messages, &config));
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn needs_compaction_returns_true_above_threshold() {
        let config = default_config(); // threshold = 85 tokens
        // 20 messages with 4-char content each: 20 * (1 + 4) = 100 > 85
        let messages: Vec<Message> = (0..20).map(|_| msg(Role::User, "abcd")).collect();
        assert!(needs_compaction(&messages, &config));
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn compact_preserves_all_when_short_enough() {
        let config = default_config(); // keep_recent = 3
        let messages = vec![msg(Role::User, "one"), msg(Role::Assistant, "two")];
        let result = compact(&messages, &config);
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.tokens_freed, 0);
        assert_eq!(result.messages_dropped, 0);
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn compact_drops_old_non_system_messages() {
        let config = default_config(); // keep_recent = 3
        let messages = vec![
            msg(Role::User, "old1"),
            msg(Role::Assistant, "old2"),
            msg(Role::User, "old3"),
            msg(Role::User, "recent1"),
            msg(Role::Assistant, "recent2"),
            msg(Role::User, "recent3"),
        ];
        let result = compact(&messages, &config);
        // 3 old messages dropped, replaced by 1 placeholder, plus 3 recent
        assert_eq!(result.messages_dropped, 3);
        assert_eq!(result.messages.len(), 4); // 1 placeholder + 3 recent
        assert_eq!(result.messages[0].content, COMPACTION_PLACEHOLDER);
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn compact_never_drops_system_messages() {
        let config = default_config(); // keep_recent = 2
        let config = CompactionConfig {
            keep_recent: 2,
            ..config
        };
        let messages = vec![
            msg(Role::System, "You are a helpful assistant."),
            msg(Role::User, "old user msg"),
            msg(Role::Assistant, "old assistant msg"),
            msg(Role::User, "recent1"),
            msg(Role::Assistant, "recent2"),
        ];
        let result = compact(&messages, &config);
        // System message preserved, 2 old non-system dropped, placeholder, 2 recent
        assert_eq!(result.messages_dropped, 2);
        // system + placeholder + 2 recent = 4
        assert_eq!(result.messages.len(), 4);
        assert!(matches!(result.messages[0].role, Role::System));
        assert_eq!(result.messages[0].content, "You are a helpful assistant.");
        assert!(matches!(result.messages[1].role, Role::System));
        assert_eq!(result.messages[1].content, COMPACTION_PLACEHOLDER);
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn compact_reports_tokens_freed() {
        let config = CompactionConfig {
            context_window: 1000,
            threshold_ratio: 0.85,
            keep_recent: 1,
        };
        let messages = vec![
            msg(Role::User, &"x".repeat(400)), // 100 + 4 = 104
            msg(Role::User, &"y".repeat(400)), // 100 + 4 = 104
            msg(Role::User, "recent"),         // 2 + 4 = 6
        ];
        let tokens_before = estimate_messages(&messages);
        let result = compact(&messages, &config);
        let tokens_after = estimate_messages(&result.messages);
        assert_eq!(result.tokens_freed, tokens_before - tokens_after);
        assert!(result.tokens_freed > 0);
        assert_eq!(result.messages_dropped, 2);
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn compact_with_only_system_messages_in_candidate_region() {
        let config = CompactionConfig {
            context_window: 1000,
            threshold_ratio: 0.85,
            keep_recent: 2,
        };
        let messages = vec![
            msg(Role::System, "sys1"),
            msg(Role::System, "sys2"),
            msg(Role::User, "recent1"),
            msg(Role::Assistant, "recent2"),
        ];
        let result = compact(&messages, &config);
        // Both system messages kept, 0 dropped, no placeholder
        assert_eq!(result.messages_dropped, 0);
        assert_eq!(result.messages.len(), 4);
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn compact_empty_messages() {
        let config = default_config();
        let messages: Vec<Message> = vec![];
        let result = compact(&messages, &config);
        assert_eq!(result.messages.len(), 0);
        assert_eq!(result.tokens_freed, 0);
        assert_eq!(result.messages_dropped, 0);
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn compact_preserves_message_order() {
        let config = CompactionConfig {
            context_window: 1000,
            threshold_ratio: 0.85,
            keep_recent: 2,
        };
        let messages = vec![
            msg(Role::System, "system prompt"),
            msg(Role::User, "old1"),
            msg(Role::Assistant, "old2"),
            msg(Role::User, "recent_user"),
            msg(Role::Assistant, "recent_assistant"),
        ];
        let result = compact(&messages, &config);
        // system, placeholder, recent_user, recent_assistant
        assert_eq!(result.messages.len(), 4);
        assert_eq!(result.messages[0].content, "system prompt");
        assert_eq!(result.messages[1].content, COMPACTION_PLACEHOLDER);
        assert_eq!(result.messages[2].content, "recent_user");
        assert_eq!(result.messages[3].content, "recent_assistant");
    }

    // rtmx:req REQ-AGENT-006
    #[test]
    fn default_config_has_expected_values() {
        let config = CompactionConfig::default();
        assert_eq!(config.context_window, 128_000);
        assert!((config.threshold_ratio - 0.85).abs() < f64::EPSILON);
        assert_eq!(config.keep_recent, 10);
    }
}
