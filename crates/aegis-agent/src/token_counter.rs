//! Token counting for messages before LLM dispatch.
//!
//! Estimates token counts using a simple heuristic (~4 characters per token),
//! which is a standard approximation for English text. This allows the agent
//! to check whether a message sequence fits within a model's context window
//! before sending the request.

use aegis_domain::ports::Message;

/// Approximate number of characters per token for English text.
const CHARS_PER_TOKEN: usize = 4;

/// Overhead tokens per message for role framing and delimiters.
/// Models typically add ~4 tokens of metadata per message (role tag,
/// separator tokens, etc.).
const MESSAGE_OVERHEAD_TOKENS: usize = 4;

/// Estimate the number of tokens in a text string.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(CHARS_PER_TOKEN)
}

/// Estimate total tokens for a slice of messages.
///
/// Accounts for both content tokens and per-message overhead (role framing,
/// separator tokens).
pub fn estimate_messages(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| estimate_tokens(&m.content) + MESSAGE_OVERHEAD_TOKENS)
        .sum()
}

/// Check whether messages fit within the given context window size.
pub fn fits_context(messages: &[Message], context_window: usize) -> bool {
    estimate_messages(messages) <= context_window
}

/// Returns how many tokens the messages exceed the context window by.
/// Returns 0 if the messages fit.
pub fn overflow_by(messages: &[Message], context_window: usize) -> usize {
    let total = estimate_messages(messages);
    total.saturating_sub(context_window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::ports::Role;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
        }
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn empty_string_is_zero_tokens() {
        assert_eq!(estimate_tokens(""), 0);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn single_char_is_one_token() {
        assert_eq!(estimate_tokens("a"), 1);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn four_chars_is_one_token() {
        assert_eq!(estimate_tokens("abcd"), 1);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn five_chars_is_two_tokens() {
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn exact_multiple_of_four() {
        // 20 chars -> 5 tokens
        assert_eq!(estimate_tokens("a]".repeat(10).as_str()), 5);
        assert_eq!(estimate_tokens(&"x".repeat(100)), 25);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn empty_messages_is_zero() {
        let messages: Vec<Message> = vec![];
        assert_eq!(estimate_messages(&messages), 0);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn single_message_includes_overhead() {
        let messages = vec![msg(Role::User, "abcd")]; // 1 content token
        // 1 content + 4 overhead = 5
        assert_eq!(estimate_messages(&messages), 5);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn multiple_messages_sum_correctly() {
        let messages = vec![
            msg(Role::User, "abcd"),      // 1 + 4 = 5
            msg(Role::Assistant, "abcd"), // 1 + 4 = 5
            msg(Role::Tool, "abcdefgh"),  // 2 + 4 = 6
        ];
        assert_eq!(estimate_messages(&messages), 16);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn fits_context_returns_true_when_within_limit() {
        let messages = vec![msg(Role::User, "abcd")]; // 5 tokens
        assert!(fits_context(&messages, 5));
        assert!(fits_context(&messages, 100));
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn fits_context_returns_false_when_over_limit() {
        let messages = vec![msg(Role::User, "abcd")]; // 5 tokens
        assert!(!fits_context(&messages, 4));
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn overflow_by_returns_zero_when_fits() {
        let messages = vec![msg(Role::User, "abcd")]; // 5 tokens
        assert_eq!(overflow_by(&messages, 5), 0);
        assert_eq!(overflow_by(&messages, 100), 0);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn overflow_by_returns_excess_tokens() {
        let messages = vec![msg(Role::User, "abcd")]; // 5 tokens
        assert_eq!(overflow_by(&messages, 3), 2);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn large_message_estimate_is_reasonable() {
        // 4000 chars -> 1000 content tokens + 4 overhead = 1004
        let messages = vec![msg(Role::User, &"x".repeat(4000))];
        assert_eq!(estimate_messages(&messages), 1004);
    }

    // rtmx:req REQ-AGENT-007
    #[test]
    fn system_message_counted_like_any_other() {
        let messages = vec![msg(Role::System, "You are a helpful assistant.")];
        let tokens = estimate_messages(&messages);
        // 28 chars -> 7 content tokens + 4 overhead = 11
        assert_eq!(tokens, 11);
    }
}
