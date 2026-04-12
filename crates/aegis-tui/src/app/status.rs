//! Status line construction methods.

use super::{App, AppPhase};

/// Format an elapsed time in seconds for human-readable display.
///
/// Returns `None` for durations under 3 seconds (to avoid flicker),
/// `"Xs"` for durations under 60 seconds, and `"Xm Ys"` for longer durations.
pub fn format_elapsed(secs: u64) -> Option<String> {
    if secs < 3 {
        None
    } else if secs < 60 {
        Some(format!("{secs}s"))
    } else {
        let m = secs / 60;
        let s = secs % 60;
        Some(format!("{m}m {s}s"))
    }
}

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
            AppPhase::ToolExecuting => {
                if let Some(start) = self.tool_start {
                    let elapsed = start.elapsed().as_secs();
                    match format_elapsed(elapsed) {
                        Some(t) => format!("executing... ({t})"),
                        None => "executing...".to_string(),
                    }
                } else {
                    "executing...".to_string()
                }
            }
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

    // rtmx:req REQ-TUI-019
    #[test]
    fn format_tokens_zero_returns_empty() {
        assert_eq!(format_tokens(0), "");
    }

    // rtmx:req REQ-TUI-019
    #[test]
    fn format_tokens_small_returns_raw() {
        assert_eq!(format_tokens(42), "42");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1), "1");
    }

    // rtmx:req REQ-TUI-019
    #[test]
    fn format_tokens_thousands_returns_k_suffix() {
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(45600), "45.6k");
        assert_eq!(format_tokens(999999), "1000.0k");
    }

    // rtmx:req REQ-TUI-019
    #[test]
    fn format_tokens_millions_returns_m_suffix() {
        assert_eq!(format_tokens(1234567), "1.2M");
        assert_eq!(format_tokens(1000000), "1.0M");
        assert_eq!(format_tokens(10500000), "10.5M");
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn format_elapsed_under_3s_returns_none() {
        assert_eq!(format_elapsed(0), None);
        assert_eq!(format_elapsed(1), None);
        assert_eq!(format_elapsed(2), None);
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn format_elapsed_seconds_returns_xs() {
        assert_eq!(format_elapsed(3), Some("3s".to_string()));
        assert_eq!(format_elapsed(15), Some("15s".to_string()));
        assert_eq!(format_elapsed(59), Some("59s".to_string()));
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn format_elapsed_minutes_returns_xm_ys() {
        assert_eq!(format_elapsed(60), Some("1m 0s".to_string()));
        assert_eq!(format_elapsed(90), Some("1m 30s".to_string()));
        assert_eq!(format_elapsed(125), Some("2m 5s".to_string()));
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn format_elapsed_boundary_at_3s() {
        assert_eq!(format_elapsed(2), None);
        assert_eq!(format_elapsed(3), Some("3s".to_string()));
    }

    // rtmx:req REQ-TUI-016
    #[test]
    fn format_elapsed_boundary_at_60s() {
        assert_eq!(format_elapsed(59), Some("59s".to_string()));
        assert_eq!(format_elapsed(60), Some("1m 0s".to_string()));
    }
}
