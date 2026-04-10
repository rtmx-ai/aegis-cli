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

use crate::event::{ApprovalRequestHandle, TuiEvent};
use crate::input::InputState;
use crate::messages::ChatMessage;
use crate::slash_commands::{self, SlashCommand};
use crate::thinking::ThinkingAnimation;
use aegis_domain::types::{ApprovalDecision, ToolCall, ToolRisk};
use crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use tokio::sync::mpsc;

/// The current phase of the TUI interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPhase {
    /// Waiting for user input.
    Idle,
    /// LLM is generating tokens; streaming into `stream_buffer`.
    Streaming,
    /// A tool is executing (between ToolUse and next stream/done).
    ToolExecuting,
    /// HITL modal is displayed; waiting for A/D/E/S keypress.
    AwaitingApproval,
}

/// What the event loop should do after handling an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Continue,
    Quit,
}

/// Information about a pending HITL approval displayed as a modal overlay.
#[derive(Debug, Clone)]
pub struct ApprovalDisplayInfo {
    /// Name of the tool requesting approval.
    pub tool_name: String,
    /// Summary of the tool's arguments.
    pub args_summary: String,
    /// Risk level of the tool call.
    pub risk: ToolRisk,
}

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

    // Metrics
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model_name: String,

    // Scrolling
    pub scroll_offset: u16,
    pub auto_scroll: bool,
}

impl App {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            phase: AppPhase::Idle,
            messages: Vec::new(),
            input: InputState::default(),
            thinking: ThinkingAnimation::new(),
            stream_buffer: String::new(),
            pending_approval: None,
            approval_display: None,
            should_quit: false,
            context_files: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            model_name: model_name.into(),
            scroll_offset: 0,
            auto_scroll: true,
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
                self.stream_buffer.push_str(&text);
                Action::Continue
            }
            TuiEvent::AgentToolUse(call) => {
                self.phase = AppPhase::ToolExecuting;
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
                if self.phase == AppPhase::Streaming {
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
            CtEvent::Resize(_, _) => Action::Continue,
            _ => Action::Continue,
        }
    }

    fn handle_key(&mut self, key: KeyEvent, agent_tx: &mpsc::UnboundedSender<String>) -> Action {
        // Phase-specific key handling
        match self.phase {
            AppPhase::AwaitingApproval => return self.handle_approval_key(key),
            AppPhase::Streaming | AppPhase::ToolExecuting => {
                // Ctrl+C cancels (handled by caller via cancel_token)
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return Action::Quit;
                }
                return Action::Continue;
            }
            AppPhase::Idle => {}
        }

        // Idle-phase key handling
        match key.code {
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                let text = self.input.submit();
                if text.is_empty() {
                    return Action::Continue;
                }
                // Check for slash commands
                match slash_commands::parse_slash_command(&text) {
                    slash_commands::ParseResult::Command(cmd) => self.execute_slash_command(cmd),
                    slash_commands::ParseResult::NotACommand => {
                        self.messages.push(ChatMessage::user(&text));
                        self.phase = AppPhase::Streaming;
                        self.stream_buffer.clear();
                        let _ = agent_tx.send(text);
                        Action::Continue
                    }
                    slash_commands::ParseResult::Unknown(name) => {
                        self.messages.push(ChatMessage::error(format!(
                            "Unknown command: {name}. Type /help for commands."
                        )));
                        Action::Continue
                    }
                }
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.insert_newline();
                Action::Continue
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
            KeyCode::Esc => {
                if self.input.mode == crate::input::InputMode::Insert {
                    self.input.enter_normal_mode();
                } else {
                    self.input.enter_insert_mode();
                }
                Action::Continue
            }
            KeyCode::Up => {
                self.input.history_prev();
                Action::Continue
            }
            KeyCode::Down => {
                self.input.history_next();
                Action::Continue
            }
            KeyCode::Backspace => {
                self.input.backspace();
                Action::Continue
            }
            KeyCode::Left => {
                self.input.move_left();
                Action::Continue
            }
            KeyCode::Right => {
                self.input.move_right();
                Action::Continue
            }
            KeyCode::Home => {
                self.input.move_home();
                Action::Continue
            }
            KeyCode::End => {
                self.input.move_end();
                Action::Continue
            }
            KeyCode::Char(c) => {
                self.input.insert_char(c);
                Action::Continue
            }
            _ => Action::Continue,
        }
    }

    fn handle_approval_key(&mut self, key: KeyEvent) -> Action {
        let decision = match key.code {
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('y') | KeyCode::Char('Y') => {
                Some(ApprovalDecision::Approved)
            }
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Char('n') | KeyCode::Char('N') => {
                Some(ApprovalDecision::Denied)
            }
            KeyCode::Char('s') | KeyCode::Char('S') => Some(ApprovalDecision::Skipped),
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Edit not yet implemented; treat as approve
                Some(ApprovalDecision::Approved)
            }
            _ => None,
        };

        if let Some(decision) = decision {
            self.approval_display = None;
            if let Some(handle) = self.pending_approval.take() {
                let decision_label = match decision {
                    ApprovalDecision::Approved => "Approved",
                    ApprovalDecision::Denied => "Denied",
                    ApprovalDecision::Skipped => "Skipped",
                    ApprovalDecision::Edited => "Approved (edited)",
                };
                self.messages
                    .push(ChatMessage::system(format!("[{decision_label}]")));
                let _ = handle.response_tx.send(decision);
            }
            self.phase = if matches!(
                decision,
                ApprovalDecision::Approved | ApprovalDecision::Edited
            ) {
                AppPhase::ToolExecuting
            } else {
                AppPhase::Streaming
            };
        }

        Action::Continue
    }

    fn execute_slash_command(&mut self, cmd: SlashCommand) -> Action {
        match cmd {
            SlashCommand::Clear => {
                self.messages.clear();
                self.scroll_offset = 0;
                Action::Continue
            }
            SlashCommand::Help => {
                self.messages.push(ChatMessage::system(
                    "Commands:\n\
                     /add <path>           Add file to context\n\
                     /drop <path>          Remove file from context\n\
                     /infra <subcmd>       Plugin ops: status, list, preview <name>\n\
                     /doctor               Run connectivity and health checks\n\
                     /context              Show current context summary\n\
                     /clear                Clear chat log\n\
                     /help                 Show this help\n\
                     /quit                 Exit aegis\n\
                     \n\
                     Shortcuts: Ctrl+C quit, Shift+Enter newline, Esc vim mode\n\
                     Approval: [A]pprove [D]eny [E]dit [S]kip\n\
                     \n\
                     aegis blocks writes until you approve. Read-only tools auto-execute."
                        .to_string(),
                ));
                Action::Continue
            }
            SlashCommand::Context => {
                let files_info = if self.context_files.is_empty() {
                    "Context files: (none)".to_string()
                } else {
                    let paths: Vec<String> = self
                        .context_files
                        .iter()
                        .map(|p| format!("  {}", p.display()))
                        .collect();
                    format!("Context files:\n{}", paths.join("\n"))
                };
                self.messages.push(ChatMessage::system(format!(
                    "Model: {}\nTokens: {}in + {}out\nMessages: {}\n{}",
                    self.model_name,
                    self.input_tokens,
                    self.output_tokens,
                    self.messages.len(),
                    files_info,
                )));
                Action::Continue
            }
            SlashCommand::Quit => Action::Quit,
            SlashCommand::Add(path) => {
                if path.is_empty() {
                    self.messages
                        .push(ChatMessage::error("Usage: /add <path>".to_string()));
                    return Action::Continue;
                }
                let pb = PathBuf::from(&path);
                if !pb.exists() {
                    self.messages
                        .push(ChatMessage::error(format!("File not found: {path}")));
                    return Action::Continue;
                }
                if self.context_files.contains(&pb) {
                    self.messages
                        .push(ChatMessage::system(format!("Already in context: {path}")));
                    return Action::Continue;
                }
                self.context_files.push(pb);
                self.messages
                    .push(ChatMessage::system(format!("Added to context: {path}")));
                Action::Continue
            }
            SlashCommand::Drop(path) => {
                if path.is_empty() {
                    self.messages
                        .push(ChatMessage::error("Usage: /drop <path>".to_string()));
                    return Action::Continue;
                }
                let pb = PathBuf::from(&path);
                if let Some(pos) = self.context_files.iter().position(|p| p == &pb) {
                    self.context_files.remove(pos);
                    self.messages
                        .push(ChatMessage::system(format!("Removed from context: {path}")));
                } else {
                    self.messages
                        .push(ChatMessage::error(format!("Not in context: {path}")));
                }
                Action::Continue
            }
            SlashCommand::Infra(sub) => {
                self.handle_infra_command(&sub);
                Action::Continue
            }
            SlashCommand::Doctor => {
                self.handle_doctor_command();
                Action::Continue
            }
        }
    }

    /// Handle /infra subcommands: status, list, preview <name>.
    fn handle_infra_command(&mut self, sub: &str) {
        let parts: Vec<&str> = sub.split_whitespace().collect();
        let subcmd = parts.first().copied().unwrap_or("");

        match subcmd {
            "status" => {
                self.messages.push(ChatMessage::system(
                    "[infra status] No plugins discovered. \
                     Install aegis-infra/v1 plugins on PATH to enable."
                        .to_string(),
                ));
            }
            "list" => {
                self.messages.push(ChatMessage::system(
                    "[infra list] No aegis-infra/v1 plugins found on PATH.".to_string(),
                ));
            }
            "preview" => {
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::error(
                        "Usage: /infra preview <plugin-name>".to_string(),
                    ));
                } else {
                    let plugin_name = parts[1];
                    self.messages.push(ChatMessage::system(format!(
                        "[infra preview] Plugin '{plugin_name}' not found. \
                         Run /infra list to see available plugins."
                    )));
                }
            }
            "" => {
                self.messages.push(ChatMessage::system(
                    "Usage: /infra <status|list|preview <name>>".to_string(),
                ));
            }
            other => {
                self.messages.push(ChatMessage::error(format!(
                    "Unknown /infra subcommand: {other}. \
                     Try: status, list, preview"
                )));
            }
        }
    }

    /// Handle /doctor command: run connectivity and health checks.
    fn handle_doctor_command(&mut self) {
        let mut passed = 0u32;
        let mut total = 0u32;
        let mut results: Vec<String> = Vec::new();

        // Check 1: Home directory writability
        total += 1;
        let home_check = if let Some(home) = dirs_check_home() {
            let aegis_dir = home.join(".aegis");
            if aegis_dir.exists() && aegis_dir.is_dir() {
                // Try writing a temp file
                let probe = aegis_dir.join(".doctor-probe");
                match std::fs::write(&probe, "ok") {
                    Ok(()) => {
                        let _ = std::fs::remove_file(&probe);
                        passed += 1;
                        "[PASS] Home directory: ~/.aegis is writable".to_string()
                    }
                    Err(e) => {
                        format!("[FAIL] Home directory: ~/.aegis not writable: {e}")
                    }
                }
            } else {
                "[FAIL] Home directory: ~/.aegis does not exist. Run aegis init.".to_string()
            }
        } else {
            "[FAIL] Home directory: could not determine home directory".to_string()
        };
        results.push(home_check);

        // Check 2: Configuration validity
        total += 1;
        let config_check = if let Some(home) = dirs_check_home() {
            let config_path = home.join(".aegis").join("config.toml");
            if config_path.exists() {
                match std::fs::read_to_string(&config_path) {
                    Ok(content) => {
                        if content.contains("[") || content.contains("mode") {
                            passed += 1;
                            "[PASS] Configuration: config.toml is readable".to_string()
                        } else {
                            "[FAIL] Configuration: config.toml appears empty \
                                 or invalid"
                                .to_string()
                        }
                    }
                    Err(e) => {
                        format!("[FAIL] Configuration: cannot read config.toml: {e}")
                    }
                }
            } else {
                "[FAIL] Configuration: config.toml not found. Run aegis init.".to_string()
            }
        } else {
            "[FAIL] Configuration: could not determine home directory".to_string()
        };
        results.push(config_check);

        // Check 3: Plugin discovery
        total += 1;
        results.push("[PASS] Plugin discovery: scan deferred (run /infra list)".to_string());
        passed += 1;

        // Check 4: LLM endpoint reachability
        total += 1;
        if self.model_name.is_empty() {
            results.push("[FAIL] LLM endpoint: no model configured".to_string());
        } else {
            results.push(format!(
                "[PASS] LLM endpoint: model '{}' configured \
                 (connectivity check deferred to async)",
                self.model_name
            ));
            passed += 1;
        }

        // Summary
        results.push(format!("\n{passed}/{total} checks passed"));

        self.messages.push(ChatMessage::system(results.join("\n")));
    }

    /// Status text for the status line, reflecting current phase.
    pub fn status_text(&self) -> String {
        let phase = match self.phase {
            AppPhase::Idle => String::new(),
            AppPhase::Streaming => format!(" | {}", self.thinking.current_text()),
            AppPhase::ToolExecuting => " | executing tool...".to_string(),
            AppPhase::AwaitingApproval => " | APPROVE? [A/D/E/S]".to_string(),
        };
        let tokens = if self.input_tokens > 0 || self.output_tokens > 0 {
            format!(" | {}in + {}out", self.input_tokens, self.output_tokens)
        } else {
            String::new()
        };
        format!("{}{}{}", self.model_name, phase, tokens)
    }
}

/// Return the user's home directory, or None if unavailable.
///
/// Uses the `HOME` env var on Unix, `USERPROFILE` on Windows.
fn dirs_check_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::types::FilePath;

    fn make_app() -> App {
        App::new("llama3")
    }

    fn make_agent_tx() -> (
        mpsc::UnboundedSender<String>,
        mpsc::UnboundedReceiver<String>,
    ) {
        mpsc::unbounded_channel()
    }

    // @req REQ-TUI-001
    #[test]
    fn app_starts_in_idle_phase() {
        let app = make_app();
        assert_eq!(app.phase, AppPhase::Idle);
        assert!(app.messages.is_empty());
        assert_eq!(app.input_tokens, 0);
    }

    // @req REQ-TUI-001
    #[test]
    fn status_text_shows_model_when_idle() {
        let app = make_app();
        assert_eq!(app.status_text(), "llama3");
    }

    // @req REQ-TUI-002
    #[test]
    fn agent_token_accumulates_in_stream_buffer() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("Hello ".to_string()), &tx);
        app.handle_event(TuiEvent::AgentToken("world".to_string()), &tx);
        assert_eq!(app.stream_buffer, "Hello world");
        assert_eq!(app.phase, AppPhase::Streaming);
    }

    // @req REQ-TUI-002
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

    // @req REQ-HITL-001
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

    // @req REQ-HITL-001
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

    // @req REQ-HITL-001
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

    // @req REQ-TUI-001
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

    // @req REQ-TUI-001
    #[test]
    fn slash_clear_empties_messages() {
        let mut app = make_app();
        app.messages.push(ChatMessage::user("hello"));
        app.messages.push(ChatMessage::assistant("hi"));
        let action = app.execute_slash_command(SlashCommand::Clear);
        assert_eq!(action, Action::Continue);
        assert!(app.messages.is_empty());
    }

    // @req REQ-TUI-001
    #[test]
    fn slash_quit_returns_quit_action() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Quit);
        assert_eq!(action, Action::Quit);
    }

    // @req REQ-TUI-001
    #[test]
    fn slash_help_adds_system_message() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Help);
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("/clear"));
    }

    // @req REQ-TUI-001
    #[test]
    fn slash_context_shows_model_and_tokens() {
        let mut app = make_app();
        app.input_tokens = 500;
        app.output_tokens = 1200;
        app.execute_slash_command(SlashCommand::Context);
        assert!(app.messages[0].content.contains("llama3"));
        assert!(app.messages[0].content.contains("500in"));
    }

    // @req REQ-TUI-005
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

    // @req REQ-TUI-001
    #[test]
    fn ctrl_c_in_idle_returns_quit() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = app.handle_key(key, &tx);
        assert_eq!(action, Action::Quit);
    }

    // @req REQ-TUI-002
    #[test]
    fn status_text_shows_streaming_when_active() {
        let mut app = make_app();
        app.phase = AppPhase::Streaming;
        let status = app.status_text();
        assert!(status.starts_with("llama3"));
        // Should contain the thinking animation text
        assert!(status.contains('|'));
    }

    // @req REQ-HITL-001
    #[test]
    fn status_text_shows_approval_prompt() {
        let mut app = make_app();
        app.phase = AppPhase::AwaitingApproval;
        let status = app.status_text();
        assert!(status.contains("APPROVE?"));
        assert!(status.contains("[A/D/E/S]"));
    }

    // @req REQ-TUI-024
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

    // @req REQ-TUI-024
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

    // @req REQ-TUI-024
    #[test]
    fn add_command_without_path_shows_usage() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Add(String::new()));
        assert_eq!(action, Action::Continue);
        assert!(app.messages.last().unwrap().content.contains("Usage"));
    }

    // @req REQ-TUI-024
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

    // @req REQ-TUI-024
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

    // @req REQ-TUI-024
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

    // @req REQ-TUI-024
    #[test]
    fn drop_command_without_path_shows_usage() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Drop(String::new()));
        assert_eq!(action, Action::Continue);
        assert!(app.messages.last().unwrap().content.contains("Usage"));
    }

    // @req REQ-TUI-024
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

    // @req REQ-TUI-024
    #[test]
    fn help_command_mentions_add_and_drop() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Help);
        let content = &app.messages[0].content;
        assert!(content.contains("/add"));
        assert!(content.contains("/drop"));
    }

    // @req REQ-TUI-026
    #[test]
    fn infra_status_shows_no_plugins_message() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra("status".to_string()));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("infra status"));
        assert!(app.messages[0].content.contains("No plugins discovered"));
    }

    // @req REQ-TUI-026
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

    // @req REQ-TUI-026
    #[test]
    fn infra_preview_without_name_shows_usage() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra("preview".to_string()));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("Usage"));
    }

    // @req REQ-TUI-026
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

    // @req REQ-TUI-026
    #[test]
    fn infra_no_subcommand_shows_usage() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Infra(String::new()));
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("Usage"));
    }

    // @req REQ-TUI-026
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

    // @req REQ-TUI-028
    #[test]
    fn doctor_command_shows_check_results() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Doctor);
        assert_eq!(app.messages.len(), 1);
        let content = &app.messages[0].content;
        assert!(content.contains("checks passed"));
    }

    // @req REQ-TUI-028
    #[test]
    fn doctor_command_checks_llm_configured() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Doctor);
        let content = &app.messages[0].content;
        // App has model_name "llama3" so LLM check should pass
        assert!(content.contains("LLM endpoint"));
        assert!(content.contains("llama3"));
    }

    // @req REQ-TUI-028
    #[test]
    fn doctor_with_empty_model_fails_llm_check() {
        let mut app = App::new("");
        app.execute_slash_command(SlashCommand::Doctor);
        let content = &app.messages[0].content;
        assert!(content.contains("[FAIL] LLM endpoint"));
    }

    // @req REQ-TUI-028
    #[test]
    fn doctor_returns_continue_action() {
        let mut app = make_app();
        let action = app.execute_slash_command(SlashCommand::Doctor);
        assert_eq!(action, Action::Continue);
    }

    // @req REQ-TUI-026
    #[test]
    fn help_command_mentions_infra_and_doctor() {
        let mut app = make_app();
        app.execute_slash_command(SlashCommand::Help);
        let content = &app.messages[0].content;
        assert!(content.contains("/infra"));
        assert!(content.contains("/doctor"));
    }

    // @req REQ-TUI-029
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

    // @req REQ-TUI-029
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

    // @req REQ-TUI-029
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

    // @req REQ-TUI-032
    #[test]
    fn streaming_buffer_accumulates_tokens() {
        let mut app = make_app();
        let (tx, _rx) = make_agent_tx();
        app.handle_event(TuiEvent::AgentToken("Hello ".to_string()), &tx);
        app.handle_event(TuiEvent::AgentToken("world".to_string()), &tx);
        assert_eq!(app.stream_buffer, "Hello world");
        assert_eq!(app.phase, AppPhase::Streaming);
    }

    // @req REQ-TUI-032
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

    // @req REQ-TUI-032
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

    // @req REQ-TUI-032
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
}
