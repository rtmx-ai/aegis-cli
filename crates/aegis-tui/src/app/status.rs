//! Status line construction methods.

use super::{App, AppPhase};

/// Format a token count for human-readable display.
///
/// - 0 -> empty string (not shown)
/// - 1-999 -> as-is: `"42"`
/// - 1000-999999 -> with k suffix: `"1.5k"`
/// - 1000000+ -> with M suffix: `"1.2M"`
pub fn format_tokens(count: u64) -> String {
    if count == 0 {
        String::new()
    } else if count < 1_000 {
        format!("{count}")
    } else if count < 1_000_000 {
        let k = count as f64 / 1_000.0;
        format!("{:.1}k", k)
    } else {
        let m = count as f64 / 1_000_000.0;
        format!("{:.1}M", m)
    }
}

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

    /// Total tokens (input + output) accumulated this session.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
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
            format!(
                " | in: {} | out: {}",
                format_tokens(info.input_tokens),
                format_tokens(info.output_tokens),
            )
        } else {
            String::new()
        };
        format!("{}{}{}", info.model, phase, tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-TUI-019
    #[test]
    fn format_tokens_zero_returns_empty() {
        assert_eq!(format_tokens(0), "");
    }

    // @req REQ-TUI-019
    #[test]
    fn format_tokens_small_returns_raw() {
        assert_eq!(format_tokens(42), "42");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1), "1");
    }

    // @req REQ-TUI-019
    #[test]
    fn format_tokens_thousands_returns_k_suffix() {
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(45600), "45.6k");
        assert_eq!(format_tokens(999999), "1000.0k");
    }

    // @req REQ-TUI-019
    #[test]
    fn format_tokens_millions_returns_m_suffix() {
        assert_eq!(format_tokens(1234567), "1.2M");
        assert_eq!(format_tokens(1000000), "1.0M");
        assert_eq!(format_tokens(10500000), "10.5M");
    }
}
