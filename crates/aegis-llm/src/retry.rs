//! Retry with exponential backoff for transient LLM API failures.

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            multiplier: 2.0,
        }
    }
}

/// Returns true if the HTTP status code indicates a transient error
/// that should be retried: 429 (rate limit), 503 (service unavailable),
/// 504 (gateway timeout). All other 4xx codes are considered client
/// errors and are not retried.
pub fn is_retryable(status_code: u16) -> bool {
    matches!(status_code, 429 | 503 | 504)
}

/// Calculate the delay in milliseconds for a given retry attempt
/// using exponential backoff: `base_delay_ms * multiplier^attempt`.
/// Attempt is zero-indexed (first retry = attempt 0).
pub fn calculate_delay(attempt: u32, config: &RetryConfig) -> u64 {
    let factor = config.multiplier.powi(attempt as i32);
    (config.base_delay_ms as f64 * factor) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-011
    #[test]
    fn default_config_values() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.base_delay_ms, 500);
        assert!((cfg.multiplier - 2.0).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn retryable_429_rate_limit() {
        assert!(is_retryable(429));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn retryable_503_service_unavailable() {
        assert!(is_retryable(503));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn retryable_504_gateway_timeout() {
        assert!(is_retryable(504));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn not_retryable_400_bad_request() {
        assert!(!is_retryable(400));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn not_retryable_401_unauthorized() {
        assert!(!is_retryable(401));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn not_retryable_403_forbidden() {
        assert!(!is_retryable(403));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn not_retryable_404_not_found() {
        assert!(!is_retryable(404));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn not_retryable_422_unprocessable() {
        assert!(!is_retryable(422));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn not_retryable_200_success() {
        assert!(!is_retryable(200));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn not_retryable_500_internal_error() {
        assert!(!is_retryable(500));
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn delay_attempt_zero() {
        let cfg = RetryConfig::default();
        // 500 * 2.0^0 = 500
        assert_eq!(calculate_delay(0, &cfg), 500);
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn delay_attempt_one() {
        let cfg = RetryConfig::default();
        // 500 * 2.0^1 = 1000
        assert_eq!(calculate_delay(1, &cfg), 1000);
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn delay_attempt_two() {
        let cfg = RetryConfig::default();
        // 500 * 2.0^2 = 2000
        assert_eq!(calculate_delay(2, &cfg), 2000);
    }

    // rtmx:req REQ-LLM-011
    #[test]
    fn delay_with_custom_config() {
        let cfg = RetryConfig {
            max_retries: 5,
            base_delay_ms: 100,
            multiplier: 3.0,
        };
        // 100 * 3.0^0 = 100
        assert_eq!(calculate_delay(0, &cfg), 100);
        // 100 * 3.0^1 = 300
        assert_eq!(calculate_delay(1, &cfg), 300);
        // 100 * 3.0^2 = 900
        assert_eq!(calculate_delay(2, &cfg), 900);
    }
}
