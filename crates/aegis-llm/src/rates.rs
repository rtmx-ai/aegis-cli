//! Static rate table for per-model token pricing.

/// Per-million-token pricing for a model.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRates {
    pub input_per_million: f64,
    pub output_per_million: f64,
}

/// Look up rates for a given provider and model.
/// Returns None for unknown models. Returns $0/$0 for local models.
pub fn get_rates(provider_kind: &str, model: &str) -> Option<ModelRates> {
    match (provider_kind, model) {
        // Local -- always free
        ("local", _) => Some(ModelRates {
            input_per_million: 0.0,
            output_per_million: 0.0,
        }),

        // GCP Vertex AI
        ("vertex", m) if m.contains("gemini") && m.contains("pro") => Some(ModelRates {
            input_per_million: 1.25,
            output_per_million: 10.0,
        }),
        ("vertex", m) if m.contains("gemini") && m.contains("flash") => Some(ModelRates {
            input_per_million: 0.15,
            output_per_million: 0.60,
        }),
        // Claude on Vertex (Model Garden)
        ("vertex", m) if m.contains("claude") && m.contains("opus") => Some(ModelRates {
            input_per_million: 15.0,
            output_per_million: 75.0,
        }),
        ("vertex", m) if m.contains("claude") && m.contains("sonnet") => Some(ModelRates {
            input_per_million: 3.0,
            output_per_million: 15.0,
        }),

        // AWS Bedrock
        ("bedrock", m) if m.contains("sonnet") => Some(ModelRates {
            input_per_million: 3.0,
            output_per_million: 15.0,
        }),
        ("bedrock", m) if m.contains("haiku") => Some(ModelRates {
            input_per_million: 0.80,
            output_per_million: 4.0,
        }),

        // Azure OpenAI
        ("azure", m) if m.contains("gpt-4.1") && m.contains("mini") => Some(ModelRates {
            input_per_million: 0.40,
            output_per_million: 1.60,
        }),
        ("azure", m) if m.contains("gpt-4.1") => Some(ModelRates {
            input_per_million: 2.0,
            output_per_million: 8.0,
        }),
        ("azure", m) if m.contains("gpt-5") => Some(ModelRates {
            input_per_million: 2.0,
            output_per_million: 8.0,
        }),
        ("azure", m) if m.contains("o3-mini") => Some(ModelRates {
            input_per_million: 1.10,
            output_per_million: 4.40,
        }),

        _ => None,
    }
}

/// Calculate cost from token counts and rates.
pub fn calculate_cost(input_tokens: u64, output_tokens: u64, rates: &ModelRates) -> f64 {
    let input_cost = (input_tokens as f64 / 1_000_000.0) * rates.input_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * rates.output_per_million;
    input_cost + output_cost
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_rate_table_returns_gemini_pro_rates() {
        let rates = get_rates("vertex", "gemini-2.5-pro").unwrap();
        assert!((rates.input_per_million - 1.25).abs() < f64::EPSILON);
        assert!((rates.output_per_million - 10.0).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_rate_table_returns_gemini_flash_rates() {
        let rates = get_rates("vertex", "gemini-2.5-flash").unwrap();
        assert!((rates.input_per_million - 0.15).abs() < f64::EPSILON);
        assert!((rates.output_per_million - 0.60).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_rate_table_returns_bedrock_sonnet_rates() {
        let rates = get_rates("bedrock", "claude-sonnet-4.5").unwrap();
        assert!((rates.input_per_million - 3.0).abs() < f64::EPSILON);
        assert!((rates.output_per_million - 15.0).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_rate_table_returns_bedrock_haiku_rates() {
        let rates = get_rates("bedrock", "claude-3-haiku").unwrap();
        assert!((rates.input_per_million - 0.80).abs() < f64::EPSILON);
        assert!((rates.output_per_million - 4.0).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_rate_table_returns_azure_gpt41_rates() {
        let rates = get_rates("azure", "gpt-4.1").unwrap();
        assert!((rates.input_per_million - 2.0).abs() < f64::EPSILON);
        assert!((rates.output_per_million - 8.0).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_rate_table_returns_azure_gpt41_mini_rates() {
        let rates = get_rates("azure", "gpt-4.1-mini").unwrap();
        assert!((rates.input_per_million - 0.40).abs() < f64::EPSILON);
        assert!((rates.output_per_million - 1.60).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_local_models_are_free() {
        let rates = get_rates("local", "llama3").unwrap();
        assert!((rates.input_per_million).abs() < f64::EPSILON);
        assert!((rates.output_per_million).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_local_models_any_name_free() {
        let rates = get_rates("local", "anything").unwrap();
        assert!((rates.input_per_million).abs() < f64::EPSILON);
        assert!((rates.output_per_million).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_unknown_model_returns_none() {
        assert!(get_rates("vertex", "nonexistent-model-xyz").is_none());
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_calculate_cost_basic() {
        let rates = get_rates("vertex", "gemini-2.5-pro").unwrap();
        let cost = calculate_cost(1_000_000, 500_000, &rates);
        // 1M input * $1.25/M + 500K output * $10.00/M = $1.25 + $5.00 = $6.25
        assert!((cost - 6.25).abs() < 1e-10);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_calculate_cost_zero_tokens() {
        let rates = get_rates("vertex", "gemini-2.5-pro").unwrap();
        let cost = calculate_cost(0, 0, &rates);
        assert!((cost).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-032
    #[test]
    fn test_calculate_cost_local_free() {
        let rates = get_rates("local", "llama3").unwrap();
        let cost = calculate_cost(1_000_000, 1_000_000, &rates);
        assert!((cost).abs() < f64::EPSILON);
    }
}
