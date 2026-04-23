//! Handler for the `/cost` slash command (REQ-TUI-064).

use crate::messages::ChatMessage;

use super::App;

impl App {
    /// Display session cost breakdown: model, tokens, cost, and per-million rates.
    pub(crate) fn handle_cost_command(&mut self) {
        let input_fmt = format_tokens(self.input_tokens);
        let output_fmt = format_tokens(self.output_tokens);

        let mut lines = vec![
            format!("Model:        {}", self.model_name),
            format!("Tokens:       {input_fmt} input / {output_fmt} output"),
            format!("Session cost: ${:.2}", self.session_cost_usd),
        ];

        if self.cost_per_m_input > 0.0 || self.cost_per_m_output > 0.0 {
            lines.push(format!(
                "Rates:        ${:.2}/M input / ${:.2}/M output",
                self.cost_per_m_input, self.cost_per_m_output,
            ));
        }

        self.messages.push(ChatMessage::system(lines.join("\n")));
    }
}

/// Format a token count as a human-readable string (e.g. 1500 -> "1.5k").
fn format_tokens(count: u64) -> String {
    if count >= 1_000 {
        let k = count as f64 / 1_000.0;
        // Strip trailing zeros: 5.0k -> "5k", 1.5k -> "1.5k"
        let formatted = format!("{k:.1}");
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        format!("{trimmed}k")
    } else {
        count.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-064
    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(999), "999");
    }

    // rtmx:req REQ-TUI-064
    #[test]
    fn test_format_tokens_thousands() {
        assert_eq!(format_tokens(1_000), "1k");
        assert_eq!(format_tokens(1_500), "1.5k");
        assert_eq!(format_tokens(45_600), "45.6k");
        assert_eq!(format_tokens(100_000), "100k");
    }
}
