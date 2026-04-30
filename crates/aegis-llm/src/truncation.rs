//! Context window truncation: last-resort safety net.
//!
//! If conversation messages still exceed the LLM context window after
//! agent-layer compaction (REQ-AGENT-006), this module drops the oldest
//! non-system messages until the total estimated token count fits within
//! the budget.
//!
//! Token estimation uses the simple heuristic of ~4 characters per token
//! plus 4 tokens of per-message overhead.

use aegis_domain::ports::{Message, Role};

/// Estimate the number of tokens in a text string.
///
/// Uses the rough heuristic of 4 characters per token, with a minimum of 1.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

/// Estimate the token cost of a single message.
///
/// Includes 4 tokens of overhead per message (role marker, delimiters).
pub fn estimate_message_tokens(msg: &Message) -> usize {
    estimate_tokens(&msg.content) + 4
}

/// The result of a truncation operation.
#[derive(Debug)]
pub struct TruncationResult {
    /// The messages that fit within the token budget.
    pub messages: Vec<Message>,
    /// How many non-system messages were dropped.
    pub dropped_count: usize,
    /// Estimated total tokens of the returned messages.
    pub estimated_tokens: usize,
}

/// Truncate conversation history to fit within `max_tokens`.
///
/// Returns a new `Vec<Message>` that fits within the budget. System messages
/// are always preserved. The oldest non-system messages are dropped first.
pub fn truncate_to_fit(messages: &[Message], max_tokens: usize) -> Vec<Message> {
    truncate(messages, max_tokens).messages
}

/// Truncate conversation history to fit within `max_tokens`, returning
/// full details including the number of dropped messages.
///
/// Algorithm:
/// 1. Separate system messages (always kept) from non-system messages.
/// 2. Calculate the token cost of all system messages.
/// 3. If system messages alone exceed the budget, return only system messages.
/// 4. Otherwise, walk non-system messages from newest to oldest, accumulating
///    until adding the next message would exceed the budget.
/// 5. Return system messages (in original order) interleaved with the
///    surviving non-system messages (in original order).
pub fn truncate(messages: &[Message], max_tokens: usize) -> TruncationResult {
    // Partition into system and non-system, preserving original indices.
    let mut system_indices: Vec<usize> = Vec::new();
    let mut nonsystem_indices: Vec<usize> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        if matches!(msg.role, Role::System) {
            system_indices.push(i);
        } else {
            nonsystem_indices.push(i);
        }
    }

    // Token cost of all system messages (always included).
    let system_tokens: usize = system_indices
        .iter()
        .map(|&i| estimate_message_tokens(&messages[i]))
        .sum();

    // If system messages alone exceed budget, return only them.
    if system_tokens >= max_tokens {
        let system_msgs: Vec<Message> = system_indices
            .iter()
            .map(|&i| messages[i].clone())
            .collect();
        return TruncationResult {
            dropped_count: nonsystem_indices.len(),
            estimated_tokens: system_tokens,
            messages: system_msgs,
        };
    }

    let mut remaining_budget = max_tokens - system_tokens;

    // Walk non-system messages from newest to oldest, selecting those that fit.
    let mut keep_nonsystem: Vec<usize> = Vec::new();
    for &i in nonsystem_indices.iter().rev() {
        let cost = estimate_message_tokens(&messages[i]);
        if cost <= remaining_budget {
            keep_nonsystem.push(i);
            remaining_budget -= cost;
        } else {
            // Once we can't fit a message, stop -- all older messages are
            // also dropped.
            break;
        }
    }
    keep_nonsystem.reverse(); // Restore original order.

    let dropped_count = nonsystem_indices.len() - keep_nonsystem.len();

    // Merge system and kept non-system indices in original order.
    let mut all_kept: Vec<usize> = Vec::new();
    all_kept.extend_from_slice(&system_indices);
    all_kept.extend_from_slice(&keep_nonsystem);
    all_kept.sort_unstable();

    let result_messages: Vec<Message> = all_kept.iter().map(|&i| messages[i].clone()).collect();
    let estimated_tokens: usize = result_messages.iter().map(estimate_message_tokens).sum();

    TruncationResult {
        messages: result_messages,
        dropped_count,
        estimated_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(content: &str) -> Message {
        Message {
            role: Role::System,
            content: content.to_string(),
            cache_control: None,
        }
    }

    fn user(content: &str) -> Message {
        Message {
            role: Role::User,
            content: content.to_string(),
            cache_control: None,
        }
    }

    fn assistant(content: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: content.to_string(),
            cache_control: None,
        }
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn estimate_tokens_basic() {
        // 12 chars -> 3 tokens
        assert_eq!(estimate_tokens("hello world!"), 3);
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn estimate_tokens_minimum_is_one() {
        assert_eq!(estimate_tokens(""), 1);
        assert_eq!(estimate_tokens("ab"), 1);
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn estimate_message_tokens_adds_overhead() {
        let msg = user("hello world!"); // 12 chars -> 3 tokens + 4 overhead = 7
        assert_eq!(estimate_message_tokens(&msg), 7);
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_no_op_when_within_budget() {
        let msgs = vec![sys("system prompt"), user("hello"), assistant("hi")];
        let result = truncate(&msgs, 10_000);
        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.dropped_count, 0);
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_preserves_system_messages() {
        // System message: "system" = 6 chars -> 1 token + 4 = 5 tokens
        // User messages: "aaaa" = 4 chars -> 1 token + 4 = 5 tokens each
        // Budget: 10 tokens => system (5) + one user (5) = 10
        let msgs = vec![sys("syst"), user("aaaa"), user("bbbb")];
        let result = truncate(&msgs, 10);
        assert_eq!(result.dropped_count, 1);
        assert_eq!(result.messages.len(), 2);
        // System message preserved
        assert!(matches!(result.messages[0].role, Role::System));
        // Most recent user message preserved (bbbb), oldest dropped (aaaa)
        assert_eq!(result.messages[1].content, "bbbb");
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_drops_oldest_non_system_first() {
        // 3 user messages, budget enough for system + 2 newest
        let msgs = vec![
            sys("syst"),      // 5 tokens
            user("msg1msg1"), // 2 + 4 = 6 tokens
            user("msg2msg2"), // 6 tokens
            user("msg3msg3"), // 6 tokens
        ];
        // Budget: 5 + 6 + 6 = 17 => fits system + msg2 + msg3, drops msg1
        let result = truncate(&msgs, 17);
        assert_eq!(result.dropped_count, 1);
        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.messages[1].content, "msg2msg2");
        assert_eq!(result.messages[2].content, "msg3msg3");
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_system_exceeds_budget_returns_only_system() {
        let msgs = vec![sys("a]very long system prompt that is huge"), user("hi")];
        // System: 38 chars -> 9 tokens + 4 = 13 tokens. Budget = 5.
        let result = truncate(&msgs, 5);
        assert_eq!(result.messages.len(), 1);
        assert!(matches!(result.messages[0].role, Role::System));
        assert_eq!(result.dropped_count, 1);
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_empty_messages() {
        let result = truncate(&[], 1000);
        assert_eq!(result.messages.len(), 0);
        assert_eq!(result.dropped_count, 0);
        assert_eq!(result.estimated_tokens, 0);
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_to_fit_returns_only_messages() {
        let msgs = vec![sys("syst"), user("aaaa"), user("bbbb")];
        let result = truncate_to_fit(&msgs, 10);
        assert_eq!(result.len(), 2);
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_preserves_message_order() {
        let msgs = vec![
            sys("syst"),
            user("first"),
            assistant("reply"),
            user("second"),
            assistant("reply2"),
        ];
        // Large budget -- no truncation, just verify order.
        let result = truncate(&msgs, 100_000);
        assert_eq!(result.messages.len(), 5);
        assert!(matches!(result.messages[0].role, Role::System));
        assert!(matches!(result.messages[1].role, Role::User));
        assert!(matches!(result.messages[2].role, Role::Assistant));
        assert!(matches!(result.messages[3].role, Role::User));
        assert!(matches!(result.messages[4].role, Role::Assistant));
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_multiple_system_messages_all_preserved() {
        let msgs = vec![sys("sys1"), user("u1u1"), sys("sys2"), user("u2u2")];
        // sys1: 5, sys2: 5, each user: 5. Budget = 15 => all fit.
        let result = truncate(&msgs, 15);
        assert_eq!(result.dropped_count, 1);
        // Budget 15: sys1(5) + sys2(5) = 10. Remaining = 5. Newest user u2 (5) fits.
        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.messages[2].content, "u2u2");
    }

    // rtmx:req REQ-LLM-018
    #[test]
    fn truncate_result_estimated_tokens_is_accurate() {
        let msgs = vec![sys("syst"), user("testtest")]; // sys: 5, user: 2+4=6
        let result = truncate(&msgs, 100);
        assert_eq!(result.estimated_tokens, 11);
    }
}
