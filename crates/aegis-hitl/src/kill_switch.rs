//! Kill switch for emergency agent halt.
//!
//! Provides a shared atomic flag (Ctrl+K from TUI) and a function to
//! drain pending approval requests as Denied. Integrates with the
//! `CancellationToken` pattern from aegis-agent.
//! rtmx:req REQ-HITL-011
//! rtmx:req REQ-HITL-012

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aegis_domain::types::ApprovalDecision;

use crate::approval::ApprovalRequest;

/// Kill switch state -- shared between TUI input handler and agent loop.
///
/// When activated (Ctrl+K), the agent loop must stop iterating and all
/// pending approval requests must be drained and auto-denied.
#[derive(Debug, Clone)]
pub struct KillSwitch {
    activated: Arc<AtomicBool>,
}

impl Default for KillSwitch {
    fn default() -> Self {
        Self::new()
    }
}

impl KillSwitch {
    /// Create a new kill switch in the inactive state.
    pub fn new() -> Self {
        Self {
            activated: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Activate the kill switch (called from TUI on Ctrl+K).
    pub fn activate(&self) {
        self.activated.store(true, Ordering::Release);
        tracing::warn!("Kill switch activated (REQ-HITL-011)");
    }

    /// Check if kill switch has been activated.
    pub fn is_activated(&self) -> bool {
        self.activated.load(Ordering::Acquire)
    }

    /// Reset the kill switch (for next session).
    pub fn reset(&self) {
        self.activated.store(false, Ordering::Release);
        tracing::info!("Kill switch reset");
    }
}

/// Flush all pending approval requests as Denied.
///
/// Drains the receiver channel and responds `Denied` to each pending
/// request. Returns the count of flushed requests.
pub async fn flush_pending_approvals(
    receiver: &mut tokio::sync::mpsc::Receiver<ApprovalRequest>,
) -> usize {
    let mut count = 0;
    while let Ok(request) = receiver.try_recv() {
        let _ = request.response_tx.send(ApprovalDecision::Denied);
        tracing::info!(
            tool = %request.description,
            "Flushed pending approval as Denied (REQ-HITL-012)"
        );
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;
    use aegis_domain::types::ToolCall;
    use tokio::sync::{mpsc, oneshot};

    // rtmx:req REQ-HITL-011
    #[test]
    fn kill_switch_starts_inactive() {
        let ks = KillSwitch::new();
        assert!(!ks.is_activated());
    }

    // rtmx:req REQ-HITL-011
    #[test]
    fn activate_sets_flag() {
        let ks = KillSwitch::new();
        ks.activate();
        assert!(ks.is_activated());
    }

    // rtmx:req REQ-HITL-011
    #[test]
    fn reset_clears_flag() {
        let ks = KillSwitch::new();
        ks.activate();
        assert!(ks.is_activated());
        ks.reset();
        assert!(!ks.is_activated());
    }

    // rtmx:req REQ-HITL-011
    #[test]
    fn kill_switch_is_clone_safe() {
        let ks = KillSwitch::new();
        let clone = ks.clone();
        ks.activate();
        assert!(clone.is_activated());
    }

    /// Helper to create a pending approval request in a channel.
    fn make_pending_request(
        tx: &mpsc::Sender<ApprovalRequest>,
    ) -> oneshot::Receiver<ApprovalDecision> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = ApprovalRequest {
            tool_call: ToolCall::WriteFile {
                path: FilePath::new_unchecked("test.rs"),
                content: "code".to_string(),
            },
            description: "Write to test.rs".to_string(),
            response_tx: resp_tx,
        };
        tx.try_send(req).unwrap();
        resp_rx
    }

    // rtmx:req REQ-HITL-012
    #[tokio::test]
    async fn flush_drains_pending_requests() {
        let (tx, mut rx) = mpsc::channel(16);

        let resp1 = make_pending_request(&tx);
        let resp2 = make_pending_request(&tx);

        flush_pending_approvals(&mut rx).await;

        // Both requests should have been denied.
        assert_eq!(resp1.await.unwrap(), ApprovalDecision::Denied);
        assert_eq!(resp2.await.unwrap(), ApprovalDecision::Denied);
    }

    // rtmx:req REQ-HITL-012
    #[tokio::test]
    async fn flush_returns_count() {
        let (tx, mut rx) = mpsc::channel(16);

        let _resp1 = make_pending_request(&tx);
        let _resp2 = make_pending_request(&tx);
        let _resp3 = make_pending_request(&tx);

        let count = flush_pending_approvals(&mut rx).await;
        assert_eq!(count, 3);
    }

    // rtmx:req REQ-HITL-012
    #[tokio::test]
    async fn flush_empty_channel_returns_zero() {
        let (_tx, mut rx) = mpsc::channel::<ApprovalRequest>(16);
        let count = flush_pending_approvals(&mut rx).await;
        assert_eq!(count, 0);
    }
}
