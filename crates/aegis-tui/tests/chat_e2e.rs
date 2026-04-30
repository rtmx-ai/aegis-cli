//! End-to-end tests that wire the TUI harness with the agent loop.
//!
//! These tests exercise the full path: user input -> agent loop (mock LLM) ->
//! stream events -> TUI state updates. They verify that the TUI correctly
//! reflects streaming responses, tool calls, and HITL approval flows.

use aegis_agent::loop_runner::{AgentConfig, AgentLoop};
use aegis_domain::ports::StreamEvent;
use aegis_domain::types::{FilePath, ToolCall};
use aegis_test_support::mock_executor::MockToolExecutor;
use aegis_test_support::mock_filter::MockSecurityFilter;
use aegis_test_support::mock_gate::MockApprovalGate;
use aegis_test_support::mock_ledger::MockAuditLedger;
use aegis_test_support::mock_provider::MockLlmProvider;
use aegis_tui::app::{App, AppPhase};
use aegis_tui::event::TuiEvent;
use aegis_tui::messages::MessageKind;
use tokio::sync::mpsc;

/// Bridge StreamEvent from the agent loop into TuiEvent for the App.
fn stream_event_to_tui_event(event: StreamEvent) -> TuiEvent {
    match event {
        StreamEvent::Token(text) => TuiEvent::AgentToken(text),
        StreamEvent::ToolUse(call) => TuiEvent::AgentToolUse(call),
        StreamEvent::Done {
            input_tokens,
            output_tokens,
        } => TuiEvent::AgentDone {
            input_tokens,
            output_tokens,
        },
        StreamEvent::Error(msg) => TuiEvent::AgentError(msg),
        StreamEvent::RetryableError { message, .. } => TuiEvent::AgentError(message),
    }
}

// rtmx:req REQ-TEST-026
#[tokio::test]
async fn test_interactive_chat_with_streaming() {
    // 1. Set up mock provider with a multi-token streaming response.
    let provider = MockLlmProvider::new();
    provider.queue_response(vec![
        StreamEvent::Token("Hello".to_string()),
        StreamEvent::Token(", ".to_string()),
        StreamEvent::Token("world!".to_string()),
        StreamEvent::Done {
            input_tokens: 12,
            output_tokens: 3,
        },
    ]);

    // 2. Create event sink channel for agent -> TUI bridge.
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<StreamEvent>();

    // 3. Build the agent loop with mock components and event sink.
    let agent = AgentLoop::new(
        provider,
        MockApprovalGate::always_approve(),
        MockToolExecutor::new(),
        MockAuditLedger::new(),
        MockSecurityFilter,
        AgentConfig {
            max_iterations: 10,
            system_prompt: "You are a test assistant.".to_string(),
            is_local_provider: false,
        },
    )
    .with_event_sink(event_tx);

    // 4. Create the TUI App.
    let mut app = App::new("test-model");
    let (agent_tx, _agent_rx) = mpsc::unbounded_channel::<String>();

    // 5. Simulate user submitting "hello" -- add user message to App.
    app.messages
        .push(aegis_tui::messages::ChatMessage::user("hello"));

    // 6. Run the agent loop in a background task.
    let agent_handle = tokio::spawn(async move { agent.run("hello").await });

    // 7. Drain stream events from agent and feed them to TUI App.
    //    We collect events until the agent task completes.
    let mut received_tokens = Vec::new();
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(ref stream_event) => {
                        if let StreamEvent::Token(t) = stream_event {
                            received_tokens.push(t.clone());
                        }
                        let tui_event = stream_event_to_tui_event(
                            stream_event.clone(),
                        );
                        app.handle_event(tui_event, &agent_tx);
                    }
                    None => break, // Channel closed, agent done.
                }
            }
            _ = tokio::task::yield_now() => {}
        }
        // Check if agent finished (channel will close).
        if agent_handle.is_finished() {
            // Drain remaining events.
            while let Ok(event) = event_rx.try_recv() {
                if let StreamEvent::Token(t) = &event {
                    received_tokens.push(t.clone());
                }
                let tui_event = stream_event_to_tui_event(event);
                app.handle_event(tui_event, &agent_tx);
            }
            break;
        }
    }

    // 8. Verify the agent completed successfully.
    let result = agent_handle.await.unwrap().unwrap();
    assert_eq!(result.response, "Hello, world!");
    assert_eq!(result.input_tokens, 12);
    assert_eq!(result.output_tokens, 3);

    // 9. Verify streaming tokens were received.
    assert_eq!(received_tokens, vec!["Hello", ", ", "world!"]);

    // 10. Verify TUI state: user message + assistant message in chat log.
    assert_eq!(app.phase, AppPhase::Idle);
    assert!(
        app.stream_buffer.is_empty(),
        "stream buffer should be flushed"
    );
    assert!(
        app.messages.len() >= 2,
        "expected at least 2 messages (user + assistant), got {}",
        app.messages.len()
    );

    // First message is the user message we injected.
    assert_eq!(app.messages[0].kind, MessageKind::User);
    assert_eq!(app.messages[0].content, "hello");

    // Last message should be the assembled assistant response.
    let last = app.messages.last().unwrap();
    assert_eq!(last.kind, MessageKind::Assistant);
    assert_eq!(last.content, "Hello, world!");

    // Token counts should be accumulated.
    assert_eq!(app.input_tokens, 12);
    assert_eq!(app.output_tokens, 3);
}

// rtmx:req REQ-TEST-027
#[tokio::test]
async fn test_hitl_approval_approve_path() {
    // 1. Set up mock provider: first response proposes a write_file tool call,
    //    second response gives the final answer after tool result.
    let provider = MockLlmProvider::new();
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "fn main() {}".to_string(),
        }),
        StreamEvent::Done {
            input_tokens: 15,
            output_tokens: 5,
        },
    ]);
    provider.queue_response(vec![
        StreamEvent::Token("File written successfully.".to_string()),
        StreamEvent::Done {
            input_tokens: 30,
            output_tokens: 4,
        },
    ]);

    // 2. Wire up with auto-approve gate so the tool executes.
    let executor = MockToolExecutor::new();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<StreamEvent>();

    let agent = AgentLoop::new(
        provider,
        MockApprovalGate::always_approve(),
        executor,
        MockAuditLedger::new(),
        MockSecurityFilter,
        AgentConfig {
            max_iterations: 10,
            system_prompt: "You are a test assistant.".to_string(),
            is_local_provider: false,
        },
    )
    .with_event_sink(event_tx);

    // 3. Create TUI App and simulate user message.
    let mut app = App::new("test-model");
    let (agent_tx, _agent_rx) = mpsc::unbounded_channel::<String>();
    app.messages.push(aegis_tui::messages::ChatMessage::user(
        "write a main function",
    ));

    // 4. Run agent loop.
    let agent_handle = tokio::spawn(async move { agent.run("write a main function").await });

    // 5. Drain events into TUI.
    let mut saw_tool_use = false;
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(ref stream_event) => {
                        if matches!(stream_event, StreamEvent::ToolUse(_)) {
                            saw_tool_use = true;
                        }
                        let tui_event = stream_event_to_tui_event(
                            stream_event.clone(),
                        );
                        app.handle_event(tui_event, &agent_tx);
                    }
                    None => break,
                }
            }
            _ = tokio::task::yield_now() => {}
        }
        if agent_handle.is_finished() {
            while let Ok(event) = event_rx.try_recv() {
                if matches!(event, StreamEvent::ToolUse(_)) {
                    saw_tool_use = true;
                }
                let tui_event = stream_event_to_tui_event(event);
                app.handle_event(tui_event, &agent_tx);
            }
            break;
        }
    }

    // 6. Verify the agent completed with 2 iterations (tool call + final).
    let result = agent_handle.await.unwrap().unwrap();
    assert_eq!(result.iterations, 2);
    assert_eq!(result.response, "File written successfully.");

    // 7. Verify the TUI saw the tool use event.
    assert!(saw_tool_use, "should have seen a ToolUse stream event");

    // 8. Verify TUI state reflects the full flow.
    assert_eq!(app.phase, AppPhase::Idle);

    // Should have: user message, tool_call message, assistant message.
    assert!(
        app.messages.len() >= 3,
        "expected >= 3 messages, got {}: {:?}",
        app.messages.len(),
        app.messages
            .iter()
            .map(|m| format!("{:?}: {}", m.kind, &m.content[..m.content.len().min(40)]))
            .collect::<Vec<_>>()
    );

    // Find the tool call message.
    let tool_msg = app
        .messages
        .iter()
        .find(|m| matches!(m.kind, MessageKind::ToolCall { .. }));
    assert!(
        tool_msg.is_some(),
        "should have a tool call message in the TUI"
    );
    let tool_msg = tool_msg.unwrap();
    assert!(
        tool_msg.content.contains("src/main.rs"),
        "tool call should mention the file path"
    );

    // Final assistant message.
    let last = app.messages.last().unwrap();
    assert_eq!(last.kind, MessageKind::Assistant);
    assert_eq!(last.content, "File written successfully.");

    // Token counts accumulated across both iterations.
    assert_eq!(app.input_tokens, 45);
    assert_eq!(app.output_tokens, 9);
}

// rtmx:req REQ-TEST-028
#[tokio::test]
async fn test_hitl_approval_deny_path() {
    // 1. Set up mock provider: proposes a write_file (will be denied),
    //    then gives final answer acknowledging the denial.
    let provider = MockLlmProvider::new();
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::WriteFile {
            path: FilePath::new_unchecked("danger.sh"),
            content: "rm -rf /".to_string(),
        }),
        StreamEvent::Done {
            input_tokens: 10,
            output_tokens: 5,
        },
    ]);
    provider.queue_response(vec![
        StreamEvent::Token("Understood, skipping the write.".to_string()),
        StreamEvent::Done {
            input_tokens: 25,
            output_tokens: 6,
        },
    ]);

    // 2. Wire up with auto-DENY gate.
    let executor = MockToolExecutor::new();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<StreamEvent>();

    let agent = AgentLoop::new(
        provider,
        MockApprovalGate::always_deny(),
        executor,
        MockAuditLedger::new(),
        MockSecurityFilter,
        AgentConfig {
            max_iterations: 10,
            system_prompt: "You are a test assistant.".to_string(),
            is_local_provider: false,
        },
    )
    .with_event_sink(event_tx);

    // 3. Create TUI App.
    let mut app = App::new("test-model");
    let (agent_tx, _agent_rx) = mpsc::unbounded_channel::<String>();
    app.messages
        .push(aegis_tui::messages::ChatMessage::user("delete everything"));

    // 4. Run agent loop.
    let agent_handle = tokio::spawn(async move { agent.run("delete everything").await });

    // 5. Drain events into TUI.
    let mut saw_tool_use = false;
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(ref stream_event) => {
                        if matches!(stream_event, StreamEvent::ToolUse(_)) {
                            saw_tool_use = true;
                        }
                        let tui_event = stream_event_to_tui_event(
                            stream_event.clone(),
                        );
                        app.handle_event(tui_event, &agent_tx);
                    }
                    None => break,
                }
            }
            _ = tokio::task::yield_now() => {}
        }
        if agent_handle.is_finished() {
            while let Ok(event) = event_rx.try_recv() {
                if matches!(event, StreamEvent::ToolUse(_)) {
                    saw_tool_use = true;
                }
                let tui_event = stream_event_to_tui_event(event);
                app.handle_event(tui_event, &agent_tx);
            }
            break;
        }
    }

    // 6. Verify the agent completed -- the tool was denied, LLM continued.
    let result = agent_handle.await.unwrap().unwrap();
    assert_eq!(result.iterations, 2);
    assert_eq!(result.response, "Understood, skipping the write.");

    // 7. The tool use event was emitted by the LLM stream (before gate).
    assert!(saw_tool_use, "should have seen a ToolUse stream event");

    // 8. Verify TUI state.
    assert_eq!(app.phase, AppPhase::Idle);

    // The tool call message should appear in the TUI (the LLM proposed it).
    let tool_msg = app
        .messages
        .iter()
        .find(|m| matches!(m.kind, MessageKind::ToolCall { .. }));
    assert!(
        tool_msg.is_some(),
        "tool call should appear in TUI even when denied"
    );

    // Final assistant message reflects the denial.
    let last = app.messages.last().unwrap();
    assert_eq!(last.kind, MessageKind::Assistant);
    assert_eq!(last.content, "Understood, skipping the write.");

    // Token counts from both iterations.
    assert_eq!(app.input_tokens, 35);
    assert_eq!(app.output_tokens, 11);
}
