//! Token usage tracking and accumulation.
//!
//! Tracks input and output token counts across LLM requests
//! within a session. Used for cost estimation and context
//! window monitoring.

use serde::{Deserialize, Serialize};

/// Accumulated token usage for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub request_count: u32,
}

impl TokenUsage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record usage from a single LLM request.
    pub fn record(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.request_count += 1;
    }

    /// Total tokens (input + output).
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Estimated cost in USD given per-million-token rates.
    pub fn estimated_cost(&self, input_rate_per_m: f64, output_rate_per_m: f64) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * input_rate_per_m;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * output_rate_per_m;
        input_cost + output_cost
    }
}

impl std::fmt::Display for TokenUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}in + {}out = {} total ({} requests)",
            self.input_tokens,
            self.output_tokens,
            self.total(),
            self.request_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-007
    #[test]
    fn new_usage_is_zero() {
        let usage = TokenUsage::new();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.request_count, 0);
        assert_eq!(usage.total(), 0);
    }

    // rtmx:req REQ-LLM-007
    #[test]
    fn record_accumulates() {
        let mut usage = TokenUsage::new();
        usage.record(100, 50);
        usage.record(200, 75);
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 125);
        assert_eq!(usage.request_count, 2);
        assert_eq!(usage.total(), 425);
    }

    // rtmx:req REQ-LLM-007
    #[test]
    fn display_format() {
        let mut usage = TokenUsage::new();
        usage.record(1000, 500);
        let s = format!("{usage}");
        assert!(s.contains("1000in"));
        assert!(s.contains("500out"));
        assert!(s.contains("1500 total"));
        assert!(s.contains("1 requests"));
    }

    // rtmx:req REQ-LLM-008
    #[test]
    fn cost_estimation() {
        let mut usage = TokenUsage::new();
        usage.record(1_000_000, 500_000);
        // Gemini 2.5 Flash-like rates
        let cost = usage.estimated_cost(0.15, 0.60);
        // 1M * 0.15/M + 0.5M * 0.60/M = 0.15 + 0.30 = 0.45
        assert!(
            (cost - 0.45).abs() < 0.001,
            "Cost should be ~$0.45, got {cost}"
        );
    }

    // rtmx:req REQ-LLM-008
    #[test]
    fn zero_usage_zero_cost() {
        let usage = TokenUsage::new();
        assert_eq!(usage.estimated_cost(5.0, 25.0), 0.0);
    }

    // rtmx:req REQ-LLM-007
    #[test]
    fn serializes_to_json() {
        let mut usage = TokenUsage::new();
        usage.record(100, 50);
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"input_tokens\":100"));
        assert!(json.contains("\"output_tokens\":50"));
    }
}
