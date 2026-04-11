//! Application phase and action enums.

/// The current phase of the TUI interaction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AppPhase {
    /// Splash screen displayed on first launch; dismissed by keypress or timeout.
    Splash,
    /// Waiting for user input.
    #[default]
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
