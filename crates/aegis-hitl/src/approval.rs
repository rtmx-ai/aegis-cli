//! Channel-based approval gate for TUI integration.
//!
//! The agent loop sends approval requests via a channel; the TUI
//! receives them, displays the dialog, and sends back the decision.

use aegis_domain::error::DomainError;
use aegis_domain::ports::ApprovalGate;
use aegis_domain::types::*;
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

/// A request for human approval, sent from agent to TUI.
#[derive(Debug)]
pub struct ApprovalRequest {
    pub tool_call: ToolCall,
    pub description: String,
    pub response_tx: oneshot::Sender<ApprovalDecision>,
}

/// Create a linked approval gate and request receiver.
///
/// The gate implements `ApprovalGate` and sends requests to the
/// returned receiver. The TUI reads from the receiver, shows the
/// dialog, and sends the decision back via the oneshot channel.
pub fn create_approval_channel(
    buffer: usize,
) -> (ChannelApprovalGate, mpsc::Receiver<ApprovalRequest>) {
    let (tx, rx) = mpsc::channel(buffer);
    (ChannelApprovalGate { tx }, rx)
}

/// ApprovalGate implementation that sends requests via a channel.
#[derive(Clone)]
pub struct ChannelApprovalGate {
    tx: mpsc::Sender<ApprovalRequest>,
}

#[async_trait]
impl ApprovalGate for ChannelApprovalGate {
    async fn request_approval(
        &self,
        tool_call: &ToolCall,
    ) -> Result<ApprovalDecision, DomainError> {
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

        let decision = response_rx
            .await
            .map_err(|_| DomainError::Other("Approval channel closed".to_string()))?;

        tracing::info!(?decision, "HITL decision received");
        Ok(decision)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-HITL-001
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

    // @req REQ-HITL-001
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

    // @req REQ-HITL-001
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

    // @req REQ-HITL-001
    #[test]
    fn describe_read_file() {
        let call = ToolCall::ReadFile {
            path: FilePath::new_unchecked("readme.md"),
        };
        let desc = describe_tool_call(&call);
        assert!(desc.contains("Read"));
        assert!(desc.contains("readme.md"));
    }

    // @req REQ-HITL-001
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
            .send(ApprovalDecision::Approved)
            .unwrap();

        let decision = gate_handle.await.unwrap().unwrap();
        assert_eq!(decision, ApprovalDecision::Approved);
    }

    // @req REQ-HITL-001
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
        request.response_tx.send(ApprovalDecision::Denied).unwrap();

        let decision = gate_handle.await.unwrap().unwrap();
        assert_eq!(decision, ApprovalDecision::Denied);
    }

    // @req REQ-HITL-001
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

    // @req REQ-HITL-001
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
}
