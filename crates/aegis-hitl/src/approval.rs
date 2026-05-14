//! Channel-based approval gate for TUI integration.
//!
//! The agent loop sends approval requests via a channel; the TUI
//! receives them, displays the dialog, and sends back the decision.

use aegis_domain::error::DomainError;
use aegis_domain::ports::ApprovalGate;
use aegis_domain::types::*;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// A request for human approval, sent from agent to TUI.
#[derive(Debug)]
pub struct ApprovalRequest {
    pub tool_call: ToolCall,
    pub description: String,
    pub response_tx: oneshot::Sender<ApprovalResponse>,
}

/// Default HITL approval timeout (REQ-HITL-003).
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

/// Default approval queue depth (REQ-HITL-020).
///
/// When the queue is full, `request_approval` blocks the agent loop
/// until the TUI drains at least one pending approval.
pub const DEFAULT_APPROVAL_QUEUE_DEPTH: usize = 32;

/// Create a linked approval gate and request receiver.
///
/// The gate implements `ApprovalGate` and sends requests to the
/// returned receiver. The TUI reads from the receiver, shows the
/// dialog, and sends the decision back via the oneshot channel.
pub fn create_approval_channel(
    buffer: usize,
) -> (ChannelApprovalGate, mpsc::Receiver<ApprovalRequest>) {
    let (tx, rx) = mpsc::channel(buffer);
    (
        ChannelApprovalGate {
            tx,
            timeout: DEFAULT_APPROVAL_TIMEOUT,
        },
        rx,
    )
}

/// ApprovalGate implementation that sends requests via a channel.
#[derive(Clone)]
pub struct ChannelApprovalGate {
    tx: mpsc::Sender<ApprovalRequest>,
    timeout: Duration,
}

impl ChannelApprovalGate {
    /// Set a custom approval timeout (REQ-HITL-003).
    ///
    /// If the user does not respond within this duration, the approval
    /// is automatically denied with `ApprovalDecision::TimedOut`.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the configured approval timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[async_trait]
impl ApprovalGate for ChannelApprovalGate {
    async fn request_approval(
        &self,
        tool_call: &ToolCall,
    ) -> Result<ApprovalResponse, DomainError> {
        let (response_tx, response_rx) = oneshot::channel();
        let description = describe_tool_call(tool_call);

        tracing::info!(
            description = %description,
            "HITL approval requested"
        );

        self.tx
            .send(ApprovalRequest {
                tool_call: tool_call.clone(),
                description,
                response_tx,
            })
            .await
            .map_err(|_| DomainError::PermissionDenied)?;

        let response = match tokio::time::timeout(self.timeout, response_rx).await {
            Ok(Ok(r)) => {
                tracing::info!(?r.decision, "HITL decision received");
                r
            }
            Ok(Err(_)) => {
                return Err(DomainError::Other("Approval channel closed".to_string()));
            }
            Err(_) => {
                tracing::warn!(
                    timeout_secs = self.timeout.as_secs(),
                    "HITL approval timed out -- auto-denying (REQ-HITL-003)"
                );
                ApprovalResponse::simple(ApprovalDecision::TimedOut)
            }
        };

        Ok(response)
    }
}

/// Format a tool call for human-readable display in the HITL dialog.
pub fn describe_tool_call(tool_call: &ToolCall) -> String {
    match tool_call {
        ToolCall::WriteFile { path, content } => {
            let preview = if content.len() > 200 {
                format!("{}...", &content[..200])
            } else {
                content.clone()
            };
            format!("Write to {path}: {preview}")
        }
        ToolCall::RunCommand {
            command,
            timeout_secs,
        } => {
            format!("Execute: {command} (timeout: {timeout_secs}s)")
        }
        ToolCall::ReadFile { path } => {
            format!("Read {path}")
        }
        ToolCall::ListDir { path } => {
            format!("List directory {path}")
        }
        ToolCall::Grep { pattern, path } => {
            format!("Search for '{pattern}' in {path}")
        }
        ToolCall::McpTool {
            qualified_name,
            arguments,
        } => {
            format!("MCP: {qualified_name}({arguments})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-HITL-001
    #[test]
    fn describe_write_file() {
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "fn main() {}".to_string(),
        };
        let desc = describe_tool_call(&call);
        assert!(desc.contains("Write to"));
        assert!(desc.contains("src/main.rs"));
        assert!(desc.contains("fn main"));
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn describe_write_file_truncates_long_content() {
        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("big.txt"),
            content: "x".repeat(500),
        };
        let desc = describe_tool_call(&call);
        assert!(desc.contains("..."));
        assert!(desc.len() < 500);
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn describe_run_command() {
        let call = ToolCall::RunCommand {
            command: "npm test".to_string(),
            timeout_secs: 60,
        };
        let desc = describe_tool_call(&call);
        assert!(desc.contains("Execute: npm test"));
        assert!(desc.contains("60s"));
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn describe_read_file() {
        let call = ToolCall::ReadFile {
            path: FilePath::new_unchecked("readme.md"),
        };
        let desc = describe_tool_call(&call);
        assert!(desc.contains("Read"));
        assert!(desc.contains("readme.md"));
    }

    // rtmx:req REQ-HITL-001
    #[tokio::test]
    async fn channel_gate_sends_and_receives_approval() {
        let (gate, mut rx) = create_approval_channel(1);

        let gate_handle = tokio::spawn(async move {
            let call = ToolCall::WriteFile {
                path: FilePath::new_unchecked("test.rs"),
                content: "code".to_string(),
            };
            gate.request_approval(&call).await
        });

        // TUI side: receive request and respond
        let request = rx.recv().await.unwrap();
        assert!(request.description.contains("Write to"));
        request
            .response_tx
            .send(ApprovalResponse::simple(ApprovalDecision::Approved))
            .unwrap();

        let decision = gate_handle.await.unwrap().unwrap();
        assert_eq!(decision.decision, ApprovalDecision::Approved);
    }

    // rtmx:req REQ-HITL-001
    #[tokio::test]
    async fn channel_gate_handles_denial() {
        let (gate, mut rx) = create_approval_channel(1);

        let gate_handle = tokio::spawn(async move {
            let call = ToolCall::RunCommand {
                command: "rm -rf /".to_string(),
                timeout_secs: 10,
            };
            gate.request_approval(&call).await
        });

        let request = rx.recv().await.unwrap();
        assert!(request.description.contains("rm -rf"));
        request
            .response_tx
            .send(ApprovalResponse::simple(ApprovalDecision::Denied))
            .unwrap();

        let decision = gate_handle.await.unwrap().unwrap();
        assert_eq!(decision.decision, ApprovalDecision::Denied);
    }

    // rtmx:req REQ-HITL-001
    #[tokio::test]
    async fn channel_gate_errors_on_dropped_receiver() {
        let (gate, rx) = create_approval_channel(1);
        drop(rx);

        let call = ToolCall::WriteFile {
            path: FilePath::new_unchecked("test.rs"),
            content: "code".to_string(),
        };
        let result = gate.request_approval(&call).await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-HITL-001
    #[tokio::test]
    async fn channel_gate_errors_on_dropped_response() {
        let (gate, mut rx) = create_approval_channel(1);

        let gate_handle = tokio::spawn(async move {
            let call = ToolCall::WriteFile {
                path: FilePath::new_unchecked("test.rs"),
                content: "code".to_string(),
            };
            gate.request_approval(&call).await
        });

        // Receive but drop the response sender
        let request = rx.recv().await.unwrap();
        drop(request.response_tx);

        let result = gate_handle.await.unwrap();
        assert!(result.is_err());
    }

    // rtmx:req REQ-HITL-003
    #[test]
    fn default_timeout_is_60_seconds() {
        let (gate, _rx) = create_approval_channel(1);
        assert_eq!(gate.timeout(), Duration::from_secs(60));
    }

    // rtmx:req REQ-HITL-003
    #[test]
    fn with_timeout_sets_custom_duration() {
        let (gate, _rx) = create_approval_channel(1);
        let gate = gate.with_timeout(Duration::from_secs(120));
        assert_eq!(gate.timeout(), Duration::from_secs(120));
    }

    // rtmx:req REQ-HITL-003
    #[tokio::test]
    async fn approval_within_timeout_returns_decision() {
        let (gate, mut rx) = create_approval_channel(1);
        let gate = gate.with_timeout(Duration::from_secs(5));

        let gate_handle = tokio::spawn(async move {
            let call = ToolCall::WriteFile {
                path: FilePath::new_unchecked("test.rs"),
                content: "code".to_string(),
            };
            gate.request_approval(&call).await
        });

        let request = rx.recv().await.unwrap();
        request
            .response_tx
            .send(ApprovalResponse::simple(ApprovalDecision::Approved))
            .unwrap();

        let decision = gate_handle.await.unwrap().unwrap();
        assert_eq!(decision.decision, ApprovalDecision::Approved);
    }

    // rtmx:req REQ-HITL-003
    #[tokio::test]
    async fn approval_after_timeout_returns_timed_out() {
        tokio::time::pause();

        let (gate, mut rx) = create_approval_channel(1);
        let gate = gate.with_timeout(Duration::from_millis(100));

        let gate_handle = tokio::spawn(async move {
            let call = ToolCall::RunCommand {
                command: "dangerous-cmd".to_string(),
                timeout_secs: 10,
            };
            gate.request_approval(&call).await
        });

        // Receive the request but never respond
        let _request = rx.recv().await.unwrap();

        // Advance past the timeout
        tokio::time::advance(Duration::from_millis(200)).await;

        let decision = gate_handle.await.unwrap().unwrap();
        assert_eq!(decision.decision, ApprovalDecision::TimedOut);
    }

    // rtmx:req REQ-HITL-003
    #[test]
    fn timed_out_is_distinct_from_denied() {
        assert_ne!(ApprovalDecision::TimedOut, ApprovalDecision::Denied);
    }

    // rtmx:req REQ-HITL-020
    #[test]
    fn test_bounded_queue_capacity() {
        // Default queue depth should be 32
        assert_eq!(DEFAULT_APPROVAL_QUEUE_DEPTH, 32);
    }

    // rtmx:req REQ-HITL-020
    #[tokio::test]
    async fn test_bounded_queue_blocks_when_full() {
        // Create a channel with capacity 1
        let (gate, _rx) = create_approval_channel(1);

        // First send should succeed (fills the buffer)
        let gate_clone = gate.clone();
        let handle1 = tokio::spawn(async move {
            let call = ToolCall::WriteFile {
                path: FilePath::new_unchecked("a.rs"),
                content: "x".to_string(),
            };
            gate_clone.request_approval(&call).await
        });

        // Give first send time to fill the buffer
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Second send should block because buffer is full
        let gate_clone2 = gate.clone();
        let handle2 = tokio::spawn(async move {
            let call = ToolCall::WriteFile {
                path: FilePath::new_unchecked("b.rs"),
                content: "y".to_string(),
            };
            // This will block until the first is drained
            gate_clone2.request_approval(&call).await
        });

        // Verify second handle is NOT resolved yet (it's blocked)
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!handle2.is_finished(), "Second send should be blocked");

        // Clean up by dropping the gate handles
        handle1.abort();
        handle2.abort();
    }
}
