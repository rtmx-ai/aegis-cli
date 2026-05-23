//! E2E test: multi-tool chat task exercises the full agent -> TUI pipeline.
//!
//! REQ-CLI-003: The agent loop processes a user prompt that triggers multiple
//! sequential tool calls (read_file then write_file), verifying:
//!   - Both tool calls execute
//!   - The agent completes with a final text response
//!   - The TUI state reflects all intermediate steps

use aegis_agent::loop_runner::{AgentConfig, AgentLoop};
use aegis_domain::ports::StreamEvent;
use aegis_domain::types::{FilePath, ToolCall, ToolResult};
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

// rtmx:req REQ-CLI-003
#[tokio::test]
async fn test_multi_tool_chat_e2e() {
    // 1. Set up mock provider with three turns:
    //    Turn 1: tool_use(read_file, "src/main.rs")
    //    Turn 2: tool_use(write_file, "src/main.rs", "fn main() { /* updated */ }")
    //    Turn 3: text("Done! I read and updated the file.")
    let provider = MockLlmProvider::new();

    // Turn 1: LLM requests read_file
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::ReadFile {
            path: FilePath::new_unchecked("src/main.rs"),
        }),
        StreamEvent::Done {
            input_tokens: 20,
            output_tokens: 5,
        },
    ]);

    // Turn 2: LLM requests write_file
    provider.queue_response(vec![
        StreamEvent::ToolUse(ToolCall::WriteFile {
            path: FilePath::new_unchecked("src/main.rs"),
            content: "fn main() { /* updated */ }".to_string(),
        }),
        StreamEvent::Done {
            input_tokens: 40,
            output_tokens: 8,
        },
    ]);

    // Turn 3: LLM gives final text response
    provider.queue_response(vec![
        StreamEvent::Token("Done! I read and updated the file.".to_string()),
        StreamEvent::Done {
            input_tokens: 60,
            output_tokens: 10,
        },
    ]);

    // 2. Set up mock executor with canned results for read_file and write_file.
    let executor = MockToolExecutor::new();
    executor.set_result(
        "read_file",
        ToolResult::Success {
            output: "fn main() { println!(\"hello\"); }".to_string(),
        },
    );
    executor.set_result(
        "write_file",
        ToolResult::Success {
            output: "Wrote 27 bytes to src/main.rs".to_string(),
        },
    );

    // 3. Create event sink channel for agent -> TUI bridge.
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<StreamEvent>();

    // 4. Build the agent loop with mock components and event sink.
    //    Use auto-approve gate since write_file is state-mutating.
    let agent = AgentLoop::new(
        provider,
        MockApprovalGate::always_approve(),
        executor,
        MockAuditLedger::new(),
        MockSecurityFilter,
        AgentConfig {
            max_iterations: 10,
            system_prompt: "You are a test assistant.".to_string(),
            ..Default::default()
        },
    )
    .with_event_sink(event_tx);

    // 5. Create the TUI App and simulate user message.
    let mut app = App::new("test-model");
    let (agent_tx, _agent_rx) = mpsc::unbounded_channel::<String>();
    app.messages.push(aegis_tui::messages::ChatMessage::user(
        "Read src/main.rs and update it",
    ));

    // 6. Run the agent loop in a background task.
    let agent_handle =
        tokio::spawn(async move { agent.run("Read src/main.rs and update it").await });

    // 7. Drain stream events from agent and feed them to TUI App.
    let mut tool_use_count = 0;
    let mut saw_read_file = false;
    let mut saw_write_file = false;

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(ref stream_event) => {
                        if let StreamEvent::ToolUse(call) = stream_event {
                            tool_use_count += 1;
                            match call {
                                ToolCall::ReadFile { .. } => saw_read_file = true,
                                ToolCall::WriteFile { .. } => saw_write_file = true,
                                _ => {}
                            }
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
                if let StreamEvent::ToolUse(ref call) = event {
                    tool_use_count += 1;
                    match call {
                        ToolCall::ReadFile { .. } => saw_read_file = true,
                        ToolCall::WriteFile { .. } => saw_write_file = true,
                        _ => {}
                    }
                }
                let tui_event = stream_event_to_tui_event(event);
                app.handle_event(tui_event, &agent_tx);
            }
            break;
        }
    }

    // 8. Verify the agent completed successfully with 3 iterations.
    let result = agent_handle.await.unwrap().unwrap();
    assert_eq!(
        result.iterations, 3,
        "expected 3 iterations (read_file, write_file, final text)"
    );
    assert_eq!(result.response, "Done! I read and updated the file.");

    // 9. Verify both tool calls were observed.
    assert_eq!(tool_use_count, 2, "expected exactly 2 tool use events");
    assert!(saw_read_file, "must see read_file tool call");
    assert!(saw_write_file, "must see write_file tool call");

    // 10. Verify token accumulation across all 3 iterations.
    assert_eq!(result.input_tokens, 120); // 20 + 40 + 60
    assert_eq!(result.output_tokens, 23); // 5 + 8 + 10

    // 11. Verify TUI state reflects the full multi-tool flow.
    assert_eq!(app.phase, AppPhase::Idle);
    assert!(
        app.stream_buffer.is_empty(),
        "stream buffer should be flushed after completion"
    );

    // Expected messages: user, tool_call(read_file), tool_call(write_file),
    // assistant(final text). Exact count may vary by TUI implementation.
    assert!(
        app.messages.len() >= 4,
        "expected >= 4 messages (user + 2 tool calls + assistant), got {}",
        app.messages.len()
    );

    // First message is the user message.
    assert_eq!(app.messages[0].kind, MessageKind::User);
    assert_eq!(app.messages[0].content, "Read src/main.rs and update it");

    // Find both tool call messages in the TUI.
    let tool_msgs: Vec<_> = app
        .messages
        .iter()
        .filter(|m| matches!(m.kind, MessageKind::ToolCall { .. }))
        .collect();
    assert_eq!(
        tool_msgs.len(),
        2,
        "TUI should have 2 tool call messages, got {}",
        tool_msgs.len()
    );
    assert!(
        tool_msgs[0].content.contains("src/main.rs"),
        "first tool call should reference src/main.rs"
    );

    // Final assistant message.
    let last = app.messages.last().unwrap();
    assert_eq!(last.kind, MessageKind::Assistant);
    assert_eq!(last.content, "Done! I read and updated the file.");

    // Token counts accumulated in TUI state.
    assert_eq!(app.input_tokens, 120);
    assert_eq!(app.output_tokens, 23);
}
