//! Event-driven TUI integration test harness using ratatui's TestBackend.
//!
//! `TuiHarness` wraps the real production `App` (not a mock), a
//! `Terminal<TestBackend>`, and the agent input channel. Tests can drive the
//! App through synthetic key/agent events and assert against the rendered
//! frame contents or against `App` state directly.
//!
//! @req REQ-TEST-021

use aegis_domain::types::ToolCall;
use aegis_tui::app::App;
use aegis_tui::event::TuiEvent;
use crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

/// Integration-test harness that drives the real `App` against a
/// `TestBackend` terminal.
pub struct TuiHarness {
    app: App,
    terminal: Terminal<TestBackend>,
    agent_tx: mpsc::UnboundedSender<String>,
    agent_rx: mpsc::UnboundedReceiver<String>,
}

impl TuiHarness {
    /// Create a new harness with the given backend dimensions and model name.
    pub fn new(width: u16, height: u16, model: &str) -> Self {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("test backend terminal");
        let (agent_tx, agent_rx) = mpsc::unbounded_channel();
        let mut app = App::new(model);
        // Skip splash for test harness -- tests exercise post-splash behaviour
        app.phase = aegis_tui::app::AppPhase::Idle;
        Self {
            app,
            terminal,
            agent_tx,
            agent_rx,
        }
    }

    /// Send a synthetic key event through the App's terminal-event handler.
    pub fn send_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let key = KeyEvent::new(code, modifiers);
        self.app
            .handle_event(TuiEvent::Terminal(CtEvent::Key(key)), &self.agent_tx);
    }

    /// Send a streamed token from the agent.
    pub fn send_token(&mut self, text: &str) {
        self.app
            .handle_event(TuiEvent::AgentToken(text.to_string()), &self.agent_tx);
    }

    /// Send the agent-done event with token counts.
    pub fn send_done(&mut self, input: u64, output: u64) {
        self.app.handle_event(
            TuiEvent::AgentDone {
                input_tokens: input,
                output_tokens: output,
            },
            &self.agent_tx,
        );
    }

    /// Send an agent tool-use event.
    pub fn send_tool_use(&mut self, tool_call: ToolCall) {
        self.app
            .handle_event(TuiEvent::AgentToolUse(tool_call), &self.agent_tx);
    }

    /// Render the current `App` state into the test backend and return the
    /// buffer as a string for assertions.
    pub fn render(&mut self) -> String {
        let app = &self.app;
        self.terminal
            .draw(|frame| {
                let input_mode = match app.input.mode {
                    aegis_tui::input::InputMode::Insert => {
                        aegis_tui::layout::InputModeDisplay::Insert
                    }
                    aegis_tui::input::InputMode::Normal => {
                        aegis_tui::layout::InputModeDisplay::Normal
                    }
                };
                let view = aegis_tui::layout::AppState {
                    messages: app.messages.clone(),
                    input: app.input.text.clone(),
                    cursor: app.input.cursor,
                    status: app.status_info(),
                    scroll_offset: app.scroll_offset,
                    input_mode,
                    newline_hint: "Ctrl+O newline".to_string(),
                    spinner_frame: (app.tick_count % 4) as u8,
                    stream_buffer: app.stream_buffer.clone(),
                    approval_display: app.approval_display.clone(),
                    file_picker: app.file_picker.as_ref().map(|fp| {
                        aegis_tui::layout::FilePickerView {
                            query: fp.query.clone(),
                            entries: fp
                                .filtered
                                .iter()
                                .map(|e| (e.name.clone(), e.is_dir))
                                .collect(),
                            selected: fp.selected,
                        }
                    }),
                };
                aegis_tui::layout::render(frame, &view);
            })
            .expect("draw");
        self.terminal.backend().to_string()
    }

    /// Read-only access to the underlying App.
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Mutable access to the underlying App.
    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    /// Drain any pending agent-input messages produced by user submissions.
    pub fn drain_agent_input(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(msg) = self.agent_rx.try_recv() {
            out.push(msg);
        }
        out
    }
}

// @req REQ-TEST-021
#[test]
fn test_harness_renders_idle_state() {
    let mut harness = TuiHarness::new(80, 20, "llama3-il5");
    let frame = harness.render();
    assert!(
        frame.contains("llama3-il5"),
        "status line should show model name, got:\n{frame}"
    );
    insta::assert_snapshot!("idle_state", frame);
}

// @req REQ-TEST-021
#[test]
fn test_harness_streams_token_into_buffer() {
    let mut harness = TuiHarness::new(60, 15, "llama3");
    harness.send_token("Hello ");
    harness.send_token("world");
    assert_eq!(harness.app().stream_buffer, "Hello world");
    assert_eq!(harness.app().phase, aegis_tui::app::AppPhase::Streaming);
    let frame = harness.render();
    assert!(frame.contains("llama3"));
}

// @req REQ-TEST-021
#[test]
fn test_harness_user_can_type_and_submit() {
    let mut harness = TuiHarness::new(60, 15, "llama3");
    for ch in "hi".chars() {
        harness.send_key(KeyCode::Char(ch), KeyModifiers::NONE);
    }
    harness.send_key(KeyCode::Enter, KeyModifiers::NONE);

    // App should have a User message
    assert_eq!(harness.app().messages.len(), 1);
    assert_eq!(harness.app().messages[0].content, "hi");

    // And the agent_tx channel should have received the submitted text
    let drained = harness.drain_agent_input();
    assert_eq!(drained, vec!["hi".to_string()]);
}

// @req REQ-TEST-021
#[test]
fn test_harness_done_finalizes_assistant_message() {
    let mut harness = TuiHarness::new(60, 15, "llama3");
    harness.send_token("final answer");
    harness.send_done(10, 20);

    assert_eq!(harness.app().phase, aegis_tui::app::AppPhase::Idle);
    assert!(harness.app().stream_buffer.is_empty());
    assert_eq!(harness.app().messages.len(), 1);
    assert_eq!(harness.app().messages[0].content, "final answer");
    assert_eq!(harness.app().input_tokens, 10);
    assert_eq!(harness.app().output_tokens, 20);
}

// @req REQ-TEST-021
#[test]
fn test_harness_tool_use_appears_in_messages() {
    use aegis_domain::types::FilePath;
    let mut harness = TuiHarness::new(60, 15, "llama3");
    harness.send_tool_use(ToolCall::ReadFile {
        path: FilePath::new_unchecked("Cargo.toml"),
    });
    assert_eq!(harness.app().phase, aegis_tui::app::AppPhase::ToolExecuting);
    let frame = harness.render();
    assert!(frame.contains("read_file"), "frame:\n{frame}");
    assert!(frame.contains("Cargo.toml"), "frame:\n{frame}");
}
