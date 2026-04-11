//! Status line construction methods.

use super::{App, AppPhase};

impl App {
    /// Build a `StatusInfo` for the structured status line.
    pub fn status_info(&self) -> crate::layout::StatusInfo {
        let phase_detail = match self.phase {
            AppPhase::Streaming => self.thinking.current_text().to_string(),
            AppPhase::ToolExecuting => "executing...".to_string(),
            AppPhase::AwaitingApproval => "[A/D/E/S]".to_string(),
            _ => String::new(),
        };
        crate::layout::StatusInfo {
            model: self.model_name.clone(),
            phase: self.phase,
            phase_detail,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
        }
    }

    /// Legacy status text for backward compatibility with tests.
    pub fn status_text(&self) -> String {
        let info = self.status_info();
        let phase = match self.phase {
            AppPhase::Splash => return String::new(),
            AppPhase::Idle => String::new(),
            AppPhase::Streaming => format!(" | {}", self.thinking.current_text()),
            AppPhase::ToolExecuting => " | executing tool...".to_string(),
            AppPhase::AwaitingApproval => " | APPROVE? [A/D/E/S]".to_string(),
        };
        let tokens = if info.input_tokens > 0 || info.output_tokens > 0 {
            format!(" | {}in + {}out", info.input_tokens, info.output_tokens)
        } else {
            String::new()
        };
        format!("{}{}{}", info.model, phase, tokens)
    }
}
