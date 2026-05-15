//! Cancellation token for graceful Ctrl+C handling.
//!
//! The agent loop checks this token between iterations. When cancelled,
//! the loop returns a `Cancelled` error without executing pending tool calls.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A thread-safe cancellation token backed by an `AtomicBool`.
///
/// Clone is cheap (Arc-backed). Signal once, check many times.
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Create a new token in the non-cancelled state.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal cancellation. All clones see this immediately.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns `true` if cancellation has been signalled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Returns a future that resolves when cancellation is signalled.
    ///
    /// Polls every 50ms. Suitable for use in `tokio::select!` to make
    /// sleep/wait operations interruptible by cancellation.
    pub async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-AGENT-009
    #[test]
    fn new_token_is_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    // rtmx:req REQ-AGENT-009
    #[test]
    fn cancel_sets_flag() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    // rtmx:req REQ-AGENT-009
    #[test]
    fn clones_share_state() {
        let token = CancellationToken::new();
        let clone = token.clone();
        token.cancel();
        assert!(clone.is_cancelled());
    }

    // rtmx:req REQ-AGENT-009
    #[test]
    fn cancel_is_idempotent() {
        let token = CancellationToken::new();
        token.cancel();
        token.cancel();
        assert!(token.is_cancelled());
    }

    // rtmx:req REQ-AGENT-009
    #[test]
    fn default_is_not_cancelled() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
    }
}
