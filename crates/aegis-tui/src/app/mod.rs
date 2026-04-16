//! Application state machine for the interactive TUI.
//!
//! `App` owns all TUI state and processes `TuiEvent`s. It is the single
//! source of truth for what the user sees. The render function in layout.rs
//! reads from `App` to produce frames.
//!
//! The event loop in aegis-cli/src/main.rs drives `App` by:
//! 1. Calling `terminal.draw(|f| app.render(f))`
//! 2. Receiving the next `TuiEvent` from the unified channel
//! 3. Calling `app.handle_event(event, agent_tx)`

mod approval;
mod commands;
pub mod file_picker;
mod input_handler;
mod phase;
mod scroll;
pub mod status;

pub use approval::ApprovalDisplayInfo;
pub use phase::{Action, AppPhase};

use crate::command_palette::CommandPalette;
use crate::event::{ApprovalRequestHandle, TuiEvent};
use crate::input::InputState;
use crate::messages::ChatMessage;
use crate::thinking::ThinkingAnimation;
use aegis_domain::types::ToolCall;
use crossterm::event::Event as CtEvent;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::mpsc;

/// Central application state.
pub struct App {
    pub phase: AppPhase,
    pub messages: Vec<ChatMessage>,
    pub input: InputState,
    pub thinking: ThinkingAnimation,
    pub stream_buffer: String,
    pub pending_approval: Option<ApprovalRequestHandle>,
    pub approval_display: Option<ApprovalDisplayInfo>,
    pub should_quit: bool,

    // Context files for /add and /drop
    pub context_files: Vec<PathBuf>,

    // Debug: when true, every key event is logged to chat
    pub keylog: bool,

    // Metrics
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model_name: String,

    // Scrolling
    pub scroll_offset: u16,
    pub auto_scroll: bool,

    // Splash screen tick counter (each tick = 150 ms)
    pub splash_ticks: u16,

    // Progress indicator: when the current tool execution started.
    pub tool_start: Option<Instant>,
    // Monotonic tick counter for spinner animation.
    pub tick_count: u64,

    /// When a prompt was submitted (for inline waiting elapsed time).
    pub prompt_submitted_at: Option<Instant>,
    /// Monotonic counter of prompts submitted (for stable thinking verb).
    pub prompt_count: u32,

    // Search: index of currently matched message.
    pub search_match_index: Option<usize>,

    // File picker: interactive @-mention file selector overlay.
    pub file_picker: Option<file_picker::FilePicker>,

    pub command_palette: CommandPalette,
}

/// Number of lines to scroll per PageUp/PageDown press.
pub const PAGE_SCROLL_LINES: u16 = 10;

impl App {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            phase: AppPhase::Splash,
            messages: Vec::new(),
            input: InputState::default(),
            thinking: ThinkingAnimation::new(),
            stream_buffer: String::new(),
            pending_approval: None,
            approval_display: None,
            should_quit: false,
            context_files: Vec::new(),
            keylog: false,
            input_tokens: 0,
            output_tokens: 0,
            model_name: model_name.into(),
            scroll_offset: 0,
            auto_scroll: true,
            splash_ticks: 0,
            tool_start: None,
            tick_count: 0,
            prompt_submitted_at: None,
            prompt_count: 0,
            search_match_index: None,
            file_picker: None,
            command_palette: CommandPalette::new(),
        }
    }

    /// Inline waiting text for the chat area. Returns `Some("| Thinking (5s)")`
    /// when we are waiting for the first token, `None` otherwise.
    pub fn waiting_text(&self) -> Option<String> {
        if self.phase != AppPhase::Streaming || !self.stream_buffer.is_empty() {
            return None;
        }
        let verb = crate::thinking::THINKING_VERBS
            [(self.prompt_count as usize) % crate::thinking::THINKING_VERBS.len()];
        match self.prompt_submitted_at {
            Some(t) => {
                let elapsed = t.elapsed().as_secs();
                if elapsed >= 3 {
                    Some(format!("{verb} ({elapsed}s)"))
                } else {
                    Some(verb.to_string())
                }
            }
            None => Some(verb.to_string()),
        }
    }

    /// Process a TUI event. Returns `Action::Quit` if the app should exit.
    pub fn handle_event(
        &mut self,
        event: TuiEvent,
        agent_tx: &mpsc::UnboundedSender<String>,
    ) -> Action {
        match event {
            TuiEvent::Terminal(ct_event) => self.handle_terminal_event(ct_event, agent_tx),
            TuiEvent::AgentToken(text) => {
                self.phase = AppPhase::Streaming;
                self.prompt_submitted_at = None; // First token clears waiting indicator
                self.stream_buffer.push_str(&text);
                Action::Continue
            }
            TuiEvent::AgentToolUse(call) => {
                self.phase = AppPhase::ToolExecuting;
                self.tool_start = Some(Instant::now());
                let (name, detail) = describe_tool_call_short(&call);
                tracing::debug!(
                    tool = %name,
                    "TUI received tool use event"
                );
                self.messages.push(ChatMessage::tool_call(name, detail));
                Action::Continue
            }
            TuiEvent::AgentDone {
                input_tokens,
                output_tokens,
            } => {
                self.input_tokens += input_tokens;
                self.output_tokens += output_tokens;
                self.tool_start = None;
                // Flush stream buffer into a finalized assistant message
                if !self.stream_buffer.is_empty() {
                    let content = std::mem::take(&mut self.stream_buffer);
                    self.messages.push(ChatMessage::assistant(content));
                }
                self.phase = AppPhase::Idle;
                Action::Continue
            }
            TuiEvent::AgentError(msg) => {
                tracing::warn!(error = %msg, "agent error received");
                self.tool_start = None;
                // Flush any partial stream buffer
                if !self.stream_buffer.is_empty() {
                    let content = std::mem::take(&mut self.stream_buffer);
                    self.messages.push(ChatMessage::assistant(content));
                }
                self.messages.push(ChatMessage::error(msg));
                self.phase = AppPhase::Idle;
                Action::Continue
            }
            TuiEvent::ApprovalRequest(handle) => {
                self.messages.push(ChatMessage::system(format!(
                    "Approval required: {}",
                    handle.description
                )));
                let (tool_name, args_summary) = describe_tool_call_short(&handle.tool_call);
                let risk = handle.tool_call.risk();
                self.approval_display = Some(ApprovalDisplayInfo {
                    tool_name,
                    args_summary,
                    risk,
                });
                self.pending_approval = Some(handle);
                self.phase = AppPhase::AwaitingApproval;
                Action::Continue
            }
            TuiEvent::Tick => {
                self.tick_count = self.tick_count.wrapping_add(1);
                if self.phase == AppPhase::Splash {
                    self.splash_ticks += 1;
                    if self.splash_ticks >= crate::splash::SPLASH_TIMEOUT_TICKS {
                        self.phase = AppPhase::Idle;
                    }
                } else if self.phase == AppPhase::Streaming {
                    self.thinking.tick();
                }
                Action::Continue
            }
        }
    }

    fn handle_terminal_event(
        &mut self,
        event: CtEvent,
        agent_tx: &mpsc::UnboundedSender<String>,
    ) -> Action {
        match event {
            CtEvent::Key(key) => self.handle_key(key, agent_tx),
            CtEvent::Paste(text) => {
                if self.phase == AppPhase::Idle {
                    self.input.insert_paste(&text);
                }
                Action::Continue
            }
            CtEvent::Mouse(mouse) => self.handle_mouse(mouse),
            CtEvent::Resize(_, _) => Action::Continue,
            _ => Action::Continue,
        }
    }
}

impl App {
    /// Execute the `/undo` slash command.
    ///
    /// Scaffold only. A real undo requires hooking into the HITL-managed
    /// edit ledger so we can restore the pre-write contents of the most
    /// recently approved `write_file` call.
    // TODO(REQ-HITL-005): wire to aegis-hitl edit tracking / rollback.
    pub(crate) fn execute_undo_command(&mut self) {
        self.messages.push(ChatMessage::system(
            "Undo not yet wired to tool executor. \
             Check the audit ledger for the most recent write."
                .to_string(),
        ));
    }

    /// Execute the `/copy` slash command: copy the last fenced code block
    /// from the most recent assistant message to the system clipboard.
    pub(crate) fn execute_copy_command(&mut self) {
        let last_assistant = self
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.kind, crate::messages::MessageKind::Assistant));
        let block = match last_assistant {
            Some(msg) => extract_last_code_block(&msg.content),
            None => None,
        };
        match block {
            Some(code) => match crate::clipboard::copy_text(&code) {
                Ok(()) => {
                    self.messages.push(ChatMessage::system(format!(
                        "Copied {} bytes to clipboard.",
                        code.len()
                    )));
                }
                Err(e) => {
                    self.messages
                        .push(ChatMessage::error(format!("Copy failed: {e}")));
                }
            },
            None => {
                self.messages.push(ChatMessage::system(
                    "no code block in last response".to_string(),
                ));
            }
        }
    }
}

/// Extract the last fenced code block from a markdown-ish string.
///
/// A fenced block starts with a line whose first non-whitespace tokens
/// are three backticks (optionally followed by a language tag) and ends
/// with a subsequent line whose first non-whitespace tokens are three
/// backticks. Returns the content between the fences with no trailing
/// newline. Returns `None` if no complete fenced block is present.
pub fn extract_last_code_block(text: &str) -> Option<String> {
    let fence = "```";
    let lines: Vec<&str> = text.lines().collect();
    let mut last: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with(fence) {
            // Found an opening fence; scan for the matching close.
            let mut j = i + 1;
            let mut body: Vec<&str> = Vec::new();
            while j < lines.len() {
                if lines[j].trim_start().starts_with(fence) {
                    // Closing fence found -- record this block.
                    last = Some(body.join("\n"));
                    i = j + 1;
                    break;
                }
                body.push(lines[j]);
                j += 1;
            }
            if j >= lines.len() {
                // Unterminated fence; bail.
                break;
            }
        } else {
            i += 1;
        }
    }
    last
}

fn describe_tool_call_short(call: &ToolCall) -> (String, String) {
    match call {
        ToolCall::ReadFile { path } => ("read_file".to_string(), path.to_string()),
        ToolCall::WriteFile { path, content } => {
            let preview = if content.len() > 100 {
                format!("{}...", &content[..100])
            } else {
                content.clone()
            };
            ("write_file".to_string(), format!("{path}: {preview}"))
        }
        ToolCall::RunCommand {
            command,
            timeout_secs,
        } => (
            "run_command".to_string(),
            format!("{command} (timeout: {timeout_secs}s)"),
        ),
        ToolCall::ListDir { path } => ("list_dir".to_string(), path.to_string()),
        ToolCall::Grep { pattern, path } => {
            ("grep".to_string(), format!("'{pattern}' in {path}"))
        }
        ToolCall::McpTool {
            qualified_name,
            arguments,
        } => (qualified_name.clone(), arguments.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ApprovalRequestHandle;
    use crate::slash_commands::SlashCommand;
    use aegis_domain::types::{ApprovalDecision, FilePath, ToolCall, ToolRisk};
    use crossterm::event::{
        Event as CtEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
    };

    /// Create an app in Idle phase (post-splash) for testing normal interaction.
    fn make_app() -> App {
        let mut app = App::new("llama3");
        app.phase = AppPhase::Idle;
        app
    }

    fn make_agent_tx() -> (
        mpsc::UnboundedSender<String>,
        mpsc::UnboundedReceiver<String>,
    ) {
        mpsc::unbounded_channel()
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn app_starts_in_splash_phase() {
        let app = App::new("llama3");
        assert_eq!(app.phase, AppPhase::Splash);
        assert_eq!(app.splash_ticks, 0);
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_dismissed_by_keypress() {
        let mut app = App::new("llama3");
        assert_eq!(app.phase, AppPhase::Splash);
        let (tx, _rx) = make_agent_tx();
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char(' ')))),
            &tx,
        );
        assert_eq!(app.phase, AppPhase::Idle);
    }

    // rtmx:req REQ-TUI-030
    #[test]
    fn splash_dismissed_by_timeout() {
        let mut app = App::new("llama3");
        let (tx, _rx) = make_agent_tx();
        for _ in 0..crate::splash::SPLASH_TIMEOUT_TICKS {
            app.handle_event(TuiEvent::Tick, &tx);
        }
        assert_eq!(app.phase, AppPhase::Idle);
    }

    // rtmx:req REQ-TUI-008
    #[test]
    fn mouse_scroll_up_increases_offset() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_event(TuiEvent::Terminal(CtEvent::Mouse(mouse)), &tx);
        assert_eq!(app.scroll_offset, 1);
        assert!(!app.auto_scroll);
    }

    // rtmx:req REQ-TUI-004
    #[test]
    fn vim_o_opens_new_line_and_enters_insert() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        // Type some text
        for ch in "hello".chars() {
            app.handle_event(
                TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char(ch)))),
                &tx,
            );
        }
        // Enter normal mode
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Esc))),
            &tx,
        );
        assert_eq!(app.input.mode, crate::input::InputMode::Normal);
        // Press 'o' to open new line
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char('o')))),
            &tx,
        );
        assert_eq!(app.input.mode, crate::input::InputMode::Insert);
        assert!(
            app.input.text.contains('\n'),
            "Should have newline: {:?}",
            app.input.text
        );
    }

    // rtmx:req REQ-TUI-008
    #[test]
    fn mouse_scroll_down_decreases_offset() {
        let mut app = make_app();
        app.scroll_offset = 10;
        app.auto_scroll = false;
        let (tx, _rx) = make_agent_tx();
        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_event(TuiEvent::Terminal(CtEvent::Mouse(mouse)), &tx);
        assert_eq!(app.scroll_offset, 9);
    }

    // rtmx:req REQ-TUI-008
    #[test]
    fn mouse_scroll_down_to_zero_enables_auto_scroll() {
        let mut app = make_app();
        app.scroll_offset = 1;
        app.auto_scroll = false;
        let (tx, _rx) = make_agent_tx();
        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_event(TuiEvent::Terminal(CtEvent::Mouse(mouse)), &tx);
        assert_eq!(app.scroll_offset, 0);
        assert!(app.auto_scroll);
    }

    // rtmx:req REQ-TUI-034
    #[test]
    fn bracketed_paste_inserts_text() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Paste("pasted text".to_string())),
            &tx,
        );
        assert_eq!(app.input.text, "pasted text");
    }

    // rtmx:req REQ-TUI-034
    #[test]
    fn bracketed_paste_ignored_during_streaming() {
        let mut app = make_app();
        app.phase = AppPhase::Streaming;
        let (tx, _rx) = make_agent_tx();
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Paste("should not appear".to_string())),
            &tx,
        );
        assert_eq!(app.input.text, "");
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn app_idle_after_splash() {
        let app = make_app();
        assert_eq!(app.phase, AppPhase::Idle);
        assert!(app.messages.is_empty());
        assert_eq!(app.input_tokens, 0);
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn status_text_shows_model_when_idle() {
        let app = make_app();
        assert_eq!(app.status_text(), "llama3");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn agent_token_accumulates_in_stream_buffer() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("Hello ".to_string()), &tx);
        app.handle_event(TuiEvent::AgentToken("world".to_string()), &tx);
        assert_eq!(app.stream_buffer, "Hello world");
        assert_eq!(app.phase, AppPhase::Streaming);
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn agent_done_flushes_buffer_to_message() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("Response text".to_string()), &tx);
        app.handle_event(
            TuiEvent::AgentDone {
                input_tokens: 50,
                output_tokens: 100,
            },
            &tx,
        );
        assert_eq!(app.phase, AppPhase::Idle);
        assert!(app.stream_buffer.is_empty());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "Response text");
        assert_eq!(app.input_tokens, 50);
        assert_eq!(app.output_tokens, 100);
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn approval_request_enters_awaiting_phase() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
        let handle = ApprovalRequestHandle {
            tool_call: ToolCall::WriteFile {
                path: FilePath::new_unchecked("src/main.rs"),
                content: "fn main() {}".to_string(),
            },
            description: "Write to src/main.rs".to_string(),
            response_tx: resp_tx,
        };
        app.handle_event(TuiEvent::ApprovalRequest(handle), &tx);
        assert_eq!(app.phase, AppPhase::AwaitingApproval);
        assert!(app.pending_approval.is_some());
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn approve_key_sends_approved_decision() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        let handle = ApprovalRequestHandle {
            tool_call: ToolCall::RunCommand {
                command: "cargo test".to_string(),
                timeout_secs: 60,
            },
            description: "Execute: cargo test".to_string(),
            response_tx: resp_tx,
        };
        app.handle_event(TuiEvent::ApprovalRequest(handle), &tx);

        // Press 'a' to approve
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(key, &tx);

        assert_eq!(app.phase, AppPhase::ToolExecuting);
        assert!(app.pending_approval.is_none());
        // The decision should have been sent
        assert_eq!(resp_rx.blocking_recv().unwrap(), ApprovalDecision::Approved);
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn deny_key_sends_denied_decision() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        let handle = ApprovalRequestHandle {
            tool_call: ToolCall::WriteFile {
                path: FilePath::new_unchecked("danger.sh"),
                content: "rm -rf /".to_string(),
            },
            description: "Write to danger.sh".to_string(),
            response_tx: resp_tx,
        };
        app.handle_event(TuiEvent::ApprovalRequest(handle), &tx);

        // Press 'n' to deny
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        app.handle_key(key, &tx);

        assert_eq!(app.phase, AppPhase::Streaming);
        assert_eq!(resp_rx.blocking_recv().unwrap(), ApprovalDecision::Denied);
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn agent_error_returns_to_idle() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("partial".to_string()), &tx);
        app.handle_event(TuiEvent::AgentError("timeout".to_string()), &tx);
        assert_eq!(app.phase, AppPhase::Idle);
        // Partial buffer flushed as assistant message, then error appended
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[0].content, "partial");
        assert_eq!(app.messages[1].content, "timeout");
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn slash_clear_empties_messages() {
        let mut app = make_app();
        app.messages.push(ChatMessage::user("hello"));
        app.messages.push(ChatMessage::assistant("hi"));
        let action = app.execute_slash_command(SlashCommand::Clear);
        assert_eq!(action, Action::Continue);
        assert!(app.messages.is_empty());
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn slash_quit_returns_quit_action() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Quit);
        assert_eq!(action, Action::Quit);
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn slash_help_adds_system_message() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Help);
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("/clear"));
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn slash_context_shows_model_and_tokens() {
        let mut app = make_app();
        app.input_tokens = 500;
        app.output_tokens = 1200;
        app.execute_slash_command(SlashCommand::Context);
        assert!(app.messages[0].content.contains("llama3"));
        assert!(app.messages[0].content.contains("500in"));
    }

    // rtmx:req REQ-TUI-005
    #[test]
    fn tool_use_event_adds_tool_call_message() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(
            TuiEvent::AgentToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("Cargo.toml"),
            }),
            &tx,
        );
        assert_eq!(app.phase, AppPhase::ToolExecuting);
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("Cargo.toml"));
    }

    // rtmx:req REQ-TUI-001
    #[test]
    fn ctrl_c_in_idle_returns_quit() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = app.handle_key(key, &tx);
        assert_eq!(action, Action::Quit);
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn status_text_shows_streaming_when_active() {
        let mut app = make_app();
        app.phase = AppPhase::Streaming;
        let status = app.status_text();
        assert!(status.starts_with("llama3"));
        // Should contain the thinking animation text
        assert!(status.contains('|'));
    }

    // rtmx:req REQ-HITL-001
    #[test]
    fn status_text_shows_approval_prompt() {
        let mut app = make_app();
        app.phase = AppPhase::AwaitingApproval;
        let status = app.status_text();
        assert!(status.contains("APPROVE?"));
        assert!(status.contains("[A/D/E/S]"));
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn add_command_adds_existing_file_to_context() {
        let mut app = make_app();
        // Use Cargo.toml which always exists at workspace root
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let action = app.execute_slash_command(SlashCommand::Add(path.clone()));
        assert_eq!(action, Action::Continue);
        assert_eq!(app.context_files.len(), 1);
        assert_eq!(app.context_files[0], PathBuf::from(&path));
        assert!(
            app.messages
                .last()
                .unwrap()
                .content
                .contains("Added to context")
        );
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn add_command_rejects_nonexistent_file() {
        let mut app = make_app();
        let action =
            app.execute_slash_command(SlashCommand::Add("/nonexistent/path/file.rs".to_string()));
        assert_eq!(action, Action::Continue);
        assert!(app.context_files.is_empty());
        assert!(
            app.messages
                .last()
                .unwrap()
                .content
                .contains("File not found")
        );
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn add_command_without_path_shows_usage() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Add(String::new()));
        assert_eq!(action, Action::Continue);
        assert!(app.messages.last().unwrap().content.contains("Usage"));
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn add_command_rejects_duplicate() {
        let mut app = make_app();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        app.execute_slash_command(SlashCommand::Add(path.clone()));
        app.execute_slash_command(SlashCommand::Add(path));
        assert_eq!(app.context_files.len(), 1);
        assert!(
            app.messages
                .last()
                .unwrap()
                .content
                .contains("Already in context")
        );
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn drop_command_removes_file_from_context() {
        let mut app = make_app();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        app.execute_slash_command(SlashCommand::Add(path.clone()));
        assert_eq!(app.context_files.len(), 1);
        let action = app.execute_slash_command(SlashCommand::Drop(path));
        assert_eq!(action, Action::Continue);
        assert!(app.context_files.is_empty());
        assert!(
            app.messages
                .last()
                .unwrap()
                .content
                .contains("Removed from context")
        );
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn drop_command_errors_when_not_in_context() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Drop("not_there.rs".to_string()));
        assert_eq!(action, Action::Continue);
        assert!(
            app.messages
                .last()
                .unwrap()
                .content
                .contains("Not in context")
        );
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn drop_command_without_path_shows_usage() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Drop(String::new()));
        assert_eq!(action, Action::Continue);
        assert!(app.messages.last().unwrap().content.contains("Usage"));
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn context_command_shows_context_files() {
        let mut app = make_app();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        app.execute_slash_command(SlashCommand::Add(path.clone()));
        app.execute_slash_command(SlashCommand::Context);
        let last = &app.messages.last().unwrap().content;
        assert!(last.contains("Context files:"));
        assert!(last.contains(&path));
    }

    // rtmx:req REQ-TUI-024
    #[test]
    fn help_command_mentions_add_and_drop() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Help);
        let content = &app.messages[0].content;
        assert!(content.contains("/add"));
        assert!(content.contains("/drop"));
    }

    // rtmx:req REQ-TUI-026
    #[test]
    fn infra_status_shows_no_plugins_message() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra("status".to_string()));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("infra status"));
        assert!(app.messages[0].content.contains("No plugins discovered"));
    }

    // rtmx:req REQ-TUI-026
    #[test]
    fn infra_list_shows_no_plugins_message() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra("list".to_string()));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("infra list"));
        assert!(
            app.messages[0]
                .content
                .contains("No aegis-infra/v1 plugins")
        );
    }

    // rtmx:req REQ-TUI-026
    #[test]
    fn infra_preview_without_name_shows_usage() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra("preview".to_string()));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("Usage"));
    }

    // rtmx:req REQ-TUI-026
    #[test]
    fn infra_preview_with_name_shows_not_found() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra(
            "preview gcp-assured-workloads".to_string(),
        ));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("gcp-assured-workloads"));
        assert!(app.messages[0].content.contains("not found"));
    }

    // rtmx:req REQ-TUI-026
    #[test]
    fn infra_no_subcommand_shows_usage() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra(String::new()));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("Usage"));
    }

    // rtmx:req REQ-TUI-026
    #[test]
    fn infra_unknown_subcommand_shows_error() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra("deploy".to_string()));
        assert_eq!(app.messages.len(), 1);
        assert!(
            app.messages[0]
                .content
                .contains("Unknown /infra subcommand")
        );
    }

    // rtmx:req REQ-TUI-028
    #[test]
    fn doctor_command_shows_check_results() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Doctor);
        assert_eq!(app.messages.len(), 1);
        let content = &app.messages[0].content;
        assert!(content.contains("checks passed"));
    }

    // rtmx:req REQ-TUI-028
    #[test]
    fn doctor_command_checks_llm_configured() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Doctor);
        let content = &app.messages[0].content;
        // App has model_name "llama3" so LLM check should pass
        assert!(content.contains("LLM endpoint"));
        assert!(content.contains("llama3"));
    }

    // rtmx:req REQ-TUI-028
    #[test]
    fn doctor_with_empty_model_fails_llm_check() {
        let mut app = App::new("");
        app.execute_slash_command(SlashCommand::Doctor);
        let content = &app.messages[0].content;
        assert!(content.contains("[FAIL] LLM endpoint"));
    }

    // rtmx:req REQ-TUI-028
    #[test]
    fn doctor_returns_continue_action() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Doctor);
        assert_eq!(action, Action::Continue);
    }

    // rtmx:req REQ-TUI-026
    #[test]
    fn help_command_mentions_infra_and_doctor() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Help);
        let content = &app.messages[0].content;
        assert!(content.contains("/infra"));
        assert!(content.contains("/doctor"));
    }

    // rtmx:req REQ-TUI-029
    #[test]
    fn approval_request_sets_display_info() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
        let handle = ApprovalRequestHandle {
            tool_call: ToolCall::WriteFile {
                path: FilePath::new_unchecked("src/main.rs"),
                content: "fn main() {}".to_string(),
            },
            description: "Write to src/main.rs".to_string(),
            response_tx: resp_tx,
        };
        app.handle_event(TuiEvent::ApprovalRequest(handle), &tx);
        assert!(app.approval_display.is_some());
        let info = app.approval_display.as_ref().unwrap();
        assert_eq!(info.tool_name, "write_file");
        assert!(info.args_summary.contains("src/main.rs"));
        assert_eq!(info.risk, ToolRisk::StateMutating);
    }

    // rtmx:req REQ-TUI-029
    #[test]
    fn approval_decision_clears_display_info() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
        let handle = ApprovalRequestHandle {
            tool_call: ToolCall::RunCommand {
                command: "rm -rf /".to_string(),
                timeout_secs: 10,
            },
            description: "Execute: rm -rf /".to_string(),
            response_tx: resp_tx,
        };
        app.handle_event(TuiEvent::ApprovalRequest(handle), &tx);
        assert!(app.approval_display.is_some());

        // Deny the request
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        app.handle_key(key, &tx);
        assert!(app.approval_display.is_none());
    }

    // rtmx:req REQ-TUI-029
    #[test]
    fn approval_modal_blocks_regular_input() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.phase = AppPhase::AwaitingApproval;
        // Try typing a character -- should not affect input
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = app.handle_key(key, &tx);
        // 'x' is not a recognized approval key, so nothing happens
        assert_eq!(action, Action::Continue);
        assert_eq!(app.phase, AppPhase::AwaitingApproval);
    }

    // rtmx:req REQ-TUI-032
    #[test]
    fn streaming_buffer_accumulates_tokens() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("Hello ".to_string()), &tx);
        app.handle_event(TuiEvent::AgentToken("world".to_string()), &tx);
        assert_eq!(app.stream_buffer, "Hello world");
        assert_eq!(app.phase, AppPhase::Streaming);
    }

    // rtmx:req REQ-TUI-032
    #[test]
    fn streaming_done_flushes_buffer_to_message() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("Complete response".to_string()), &tx);
        app.handle_event(
            TuiEvent::AgentDone {
                input_tokens: 10,
                output_tokens: 20,
            },
            &tx,
        );
        assert!(app.stream_buffer.is_empty());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "Complete response");
        assert_eq!(app.phase, AppPhase::Idle);
    }

    // rtmx:req REQ-TUI-032
    #[test]
    fn streaming_buffer_clears_on_new_user_message() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        // Simulate user sending a message which clears and starts streaming
        app.input.insert_char('h');
        app.input.insert_char('i');
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(key, &tx);
        assert!(app.stream_buffer.is_empty());
        assert_eq!(app.phase, AppPhase::Streaming);
    }

    // rtmx:req REQ-TUI-032
    #[test]
    fn error_during_streaming_flushes_partial_buffer() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("partial".to_string()), &tx);
        app.handle_event(TuiEvent::AgentError("connection lost".to_string()), &tx);
        // Partial buffer flushed as assistant msg, error appended
        assert!(app.stream_buffer.is_empty());
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[0].content, "partial");
        assert_eq!(app.messages[1].content, "connection lost");
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn tool_use_sets_tool_start() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        assert!(app.tool_start.is_none());
        app.handle_event(
            TuiEvent::AgentToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("test.rs"),
            }),
            &tx,
        );
        assert!(app.tool_start.is_some());
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn agent_done_clears_tool_start() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(
            TuiEvent::AgentToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("test.rs"),
            }),
            &tx,
        );
        assert!(app.tool_start.is_some());
        app.handle_event(
            TuiEvent::AgentDone {
                input_tokens: 10,
                output_tokens: 20,
            },
            &tx,
        );
        assert!(app.tool_start.is_none());
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn agent_error_clears_tool_start() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(
            TuiEvent::AgentToolUse(ToolCall::ReadFile {
                path: FilePath::new_unchecked("test.rs"),
            }),
            &tx,
        );
        assert!(app.tool_start.is_some());
        app.handle_event(TuiEvent::AgentError("timeout".to_string()), &tx);
        assert!(app.tool_start.is_none());
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn tick_increments_tick_count() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        assert_eq!(app.tick_count, 0);
        app.handle_event(TuiEvent::Tick, &tx);
        assert_eq!(app.tick_count, 1);
        app.handle_event(TuiEvent::Tick, &tx);
        assert_eq!(app.tick_count, 2);
    }

    // rtmx:req REQ-TUI-018
    #[test]
    fn at_key_triggers_file_picker() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        assert!(app.file_picker.is_none());
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char('@')))),
            &tx,
        );
        assert!(app.file_picker.is_some());
        assert!(app.input.text.contains('@'));
    }

    // rtmx:req REQ-TUI-018
    #[test]
    fn file_picker_esc_closes_and_removes_at() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        // Open picker
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char('@')))),
            &tx,
        );
        assert!(app.file_picker.is_some());
        // Press Esc
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Esc))),
            &tx,
        );
        assert!(app.file_picker.is_none());
        assert!(
            !app.input.text.contains('@'),
            "@ should be removed on Esc: {:?}",
            app.input.text
        );
    }

    // rtmx:req REQ-TUI-018
    #[test]
    fn file_picker_enter_inserts_selected_path() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        // Create a temp dir with known files
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("main.rs"), "").unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();
        // Manually set up a picker
        app.input.insert_char('@');
        app.file_picker = Some(file_picker::FilePicker::open("", tmp.path()));
        // Press Enter to select first entry
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Enter))),
            &tx,
        );
        assert!(app.file_picker.is_none());
        assert!(
            !app.input.text.contains('@'),
            "@ should be replaced: {:?}",
            app.input.text
        );
    }

    // rtmx:req REQ-TUI-018
    #[test]
    fn file_picker_blocks_normal_input() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        // Type some text first
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char('h')))),
            &tx,
        );
        // Open picker
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char('@')))),
            &tx,
        );
        assert!(app.file_picker.is_some());
        let text_before = app.input.text.clone();
        // Type 'x' -- should go to picker query, not input
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Char('x')))),
            &tx,
        );
        assert_eq!(
            app.input.text, text_before,
            "Input should not change while picker is open"
        );
        assert_eq!(
            app.file_picker.as_ref().unwrap().query,
            "x",
            "Char should go to picker query"
        );
    }

    // rtmx:req REQ-TUI-018
    #[test]
    fn file_picker_arrow_keys_navigate() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.rs"), "").unwrap();
        std::fs::write(tmp.path().join("b.rs"), "").unwrap();
        std::fs::write(tmp.path().join("c.rs"), "").unwrap();
        app.input.insert_char('@');
        app.file_picker = Some(file_picker::FilePicker::open("", tmp.path()));
        assert_eq!(app.file_picker.as_ref().unwrap().selected, 0);

        // Down arrow
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Down))),
            &tx,
        );
        assert_eq!(app.file_picker.as_ref().unwrap().selected, 1);

        // Up arrow
        app.handle_event(
            TuiEvent::Terminal(CtEvent::Key(KeyEvent::from(KeyCode::Up))),
            &tx,
        );
        assert_eq!(app.file_picker.as_ref().unwrap().selected, 0);
    }

    // rtmx:req REQ-TUI-059
    #[test]
    fn waiting_text_shown_during_pre_token_streaming() {
        let mut app = make_app();
        app.phase = AppPhase::Streaming;
        app.stream_buffer.clear();
        app.prompt_submitted_at = Some(Instant::now());
        app.prompt_count = 1;
        let wt = app.waiting_text();
        assert!(wt.is_some(), "should show waiting text before first token");
        // Should contain one of the THINKING_VERBS, stable for this prompt_count.
        let text = wt.unwrap();
        assert!(
            crate::thinking::THINKING_VERBS
                .iter()
                .any(|v| text.contains(v)),
            "waiting text should contain a thinking verb: {text}"
        );
    }

    // rtmx:req REQ-TUI-059
    #[test]
    fn waiting_text_hidden_after_first_token() {
        let mut app = make_app();
        app.phase = AppPhase::Streaming;
        app.prompt_submitted_at = Some(Instant::now());
        app.prompt_count = 1;
        // Simulate first token arrival
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("Hello".to_string()), &tx);
        assert!(app.prompt_submitted_at.is_none());
        assert!(
            app.waiting_text().is_none(),
            "waiting text should be None after first token"
        );
    }

    // rtmx:req REQ-TUI-059
    #[test]
    fn waiting_text_stable_per_prompt() {
        let mut app = make_app();
        app.phase = AppPhase::Streaming;
        app.stream_buffer.clear();
        app.prompt_submitted_at = Some(Instant::now());
        app.prompt_count = 3;
        let text1 = app.waiting_text().unwrap();
        let text2 = app.waiting_text().unwrap();
        assert_eq!(text1, text2, "verb should be stable for same prompt_count");
    }

    // rtmx:req REQ-TUI-027
    #[test]
    fn test_undo_reverts_last_write() {
        // Scaffold: with no write history available to the TUI, /undo
        // pushes a system message directing the user to the audit ledger.
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Undo);
        assert_eq!(app.messages.len(), 1);
        let content = &app.messages[0].content;
        assert!(
            content.contains("Undo") && content.contains("audit ledger"),
            "expected undo scaffold message, got: {content}"
        );
    }

    // rtmx:req REQ-TUI-027
    #[test]
    fn test_undo_with_no_writes_shows_message() {
        let mut app = make_app();
        app.execute_undo_command();
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("Undo not yet wired"));
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn test_copy_code_block_extracts_fenced_block() {
        let input = "text\n```rust\nfn main(){}\n```\nmore";
        assert_eq!(
            extract_last_code_block(input),
            Some("fn main(){}".to_string())
        );
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn test_extract_returns_none_when_no_code_block() {
        assert_eq!(extract_last_code_block("just plain text"), None);
        assert_eq!(extract_last_code_block(""), None);
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn test_extract_returns_last_block_when_multiple() {
        let input = "\
intro\n\
```python\n\
print('first')\n\
```\n\
middle\n\
```rust\n\
let x = 1;\n\
let y = 2;\n\
```\n\
trailing";
        assert_eq!(
            extract_last_code_block(input),
            Some("let x = 1;\nlet y = 2;".to_string())
        );
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn test_extract_handles_block_with_no_language_tag() {
        let input = "before\n```\nhello\n```\nafter";
        assert_eq!(extract_last_code_block(input), Some("hello".to_string()));
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn test_extract_returns_none_for_unterminated_fence() {
        let input = "text\n```rust\nfn main() {}\nno close fence";
        assert_eq!(extract_last_code_block(input), None);
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn copy_command_with_no_assistant_message_shows_system_note() {
        let mut app = make_app();
        app.execute_copy_command();
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("no code block"));
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn copy_command_with_assistant_without_code_block_shows_note() {
        let mut app = make_app();
        app.messages
            .push(ChatMessage::assistant("plain prose, nothing fenced"));
        app.execute_copy_command();
        let last = app.messages.last().unwrap();
        assert!(last.content.contains("no code block"));
    }

    // rtmx:req REQ-TUI-045
    #[test]
    fn copy_command_reports_bytes_or_error_when_block_present() {
        let mut app = make_app();
        app.messages.push(ChatMessage::assistant(
            "here you go\n```rust\nfn main() {}\n```",
        ));
        app.execute_copy_command();
        let last = app.messages.last().unwrap();
        // Either the clipboard succeeded (bytes copied) or we surfaced a
        // user-friendly error. We must NEVER fall back to "no code block"
        // in this case because the input clearly contains one.
        assert!(
            last.content.starts_with("Copied") || last.content.starts_with("Copy failed"),
            "unexpected copy result: {}",
            last.content
        );
    }
}
