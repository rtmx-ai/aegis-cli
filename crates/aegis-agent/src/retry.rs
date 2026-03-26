//! Agent-level retry with exponential back-off for transient errors.
//!
//! This module provides retry classification and delay computation for the
//! agent loop. Unlike the LLM-layer retry in `aegis-llm`, this operates on
//! `DomainError` variants and covers transient failures during both LLM calls
//! and tool execution.

use std::time::Duration;

use aegis_domain::error::DomainError;

/// Configuration for agent-level retry behavior.
#[derive(Debug, Clone)]
pub struct AgentRetryConfig {
    /// Maximum number of retry attempts (default: 3).
    pub max_retries: u32,
    /// Base delay in milliseconds before the first retry (default: 1000).
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds, capping exponential growth (default: 30000).
    pub max_delay_ms: u64,
}

impl Default for AgentRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1_000,
            max_delay_ms: 30_000,
        }
    }
}

/// Outcome of a retried operation.
#[derive(Debug)]
pub enum RetryOutcome<T> {
    /// The operation succeeded on some attempt.
    Success(T),
    /// All retry attempts were exhausted.
    Exhausted {
        last_error: DomainError,
        attempts: u32,
    },
}

/// Returns `true` if the given `DomainError` represents a transient failure
/// that is safe to retry.
///
/// Retryable: `ProviderError` (network timeouts, rate limits, temporary
/// backend failures) and `Other` (catch-all that may include transient I/O).
///
/// Non-retryable: `FileBlocked` (policy), `PermissionDenied` (HITL denial),
/// `RequirementNotFound` (invalid input), `ConfigError` (misconfiguration),
/// `AuditError` (ledger integrity -- must not silently retry).
pub fn is_retryable_error(error: &DomainError) -> bool {
    matches!(
        error,
        DomainError::ProviderError { .. } | DomainError::Other(_)
    )
}

/// Compute the retry delay for the given zero-indexed attempt number.
///
/// Uses exponential back-off: `base_delay_ms * 2^attempt`, capped at
/// `max_delay_ms`. The `jitter_fraction` parameter (0.0..=1.0) adds
/// proportional jitter to avoid thundering-herd effects. A value of 0.5
/// adds up to 50% extra delay. Pass 0.0 for deterministic tests.
///
/// # Panics
///
/// Panics if `jitter_fraction` is not in the range `0.0..=1.0`.
pub fn compute_delay(attempt: u32, config: &AgentRetryConfig, jitter_fraction: f64) -> Duration {
    assert!(
        (0.0..=1.0).contains(&jitter_fraction),
        "jitter_fraction must be in 0.0..=1.0, got {jitter_fraction}"
    );

    let base = config.base_delay_ms as f64 * 2.0_f64.powi(attempt as i32);
    let capped = base.min(config.max_delay_ms as f64);
    let jittered = capped * (1.0 + jitter_fraction);
    // Re-cap after jitter so we never exceed max_delay_ms * 1.5 (at 50% jitter).
    let final_ms = jittered.min(config.max_delay_ms as f64 * 1.5);
    Duration::from_millis(final_ms as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- AgentRetryConfig defaults --

    // @req REQ-AGENT-017
    #[test]
    fn default_max_retries_is_three() {
        let cfg = AgentRetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
    }

    // @req REQ-AGENT-017
    #[test]
    fn default_base_delay_is_one_second() {
        let cfg = AgentRetryConfig::default();
        assert_eq!(cfg.base_delay_ms, 1_000);
    }

    // @req REQ-AGENT-017
    #[test]
    fn default_max_delay_is_thirty_seconds() {
        let cfg = AgentRetryConfig::default();
        assert_eq!(cfg.max_delay_ms, 30_000);
    }

    // -- is_retryable_error classification --

    // @req REQ-AGENT-017
    #[test]
    fn provider_error_is_retryable() {
        let err = DomainError::ProviderError {
            message: "connection reset".into(),
        };
        assert!(is_retryable_error(&err));
    }

    // @req REQ-AGENT-017
    #[test]
    fn other_error_is_retryable() {
        let err = DomainError::Other("transient I/O failure".into());
        assert!(is_retryable_error(&err));
    }

    // @req REQ-AGENT-017
    #[test]
    fn file_blocked_is_not_retryable() {
        let err = DomainError::FileBlocked {
            path: "/etc/shadow".into(),
        };
        assert!(!is_retryable_error(&err));
    }

    // @req REQ-AGENT-017
    #[test]
    fn permission_denied_is_not_retryable() {
        assert!(!is_retryable_error(&DomainError::PermissionDenied));
    }

    // @req REQ-AGENT-017
    #[test]
    fn requirement_not_found_is_not_retryable() {
        let err = DomainError::RequirementNotFound {
            id: "REQ-FAKE-001".into(),
        };
        assert!(!is_retryable_error(&err));
    }

    // @req REQ-AGENT-017
    #[test]
    fn config_error_is_not_retryable() {
        let err = DomainError::ConfigError {
            message: "missing API key".into(),
        };
        assert!(!is_retryable_error(&err));
    }

    // @req REQ-AGENT-017
    #[test]
    fn audit_error_is_not_retryable() {
        let err = DomainError::AuditError {
            message: "ledger corrupt".into(),
        };
        assert!(!is_retryable_error(&err));
    }

    // -- compute_delay: exponential back-off --

    // @req REQ-AGENT-017
    #[test]
    fn delay_attempt_zero_no_jitter() {
        let cfg = AgentRetryConfig::default();
        // 1000 * 2^0 = 1000 ms
        assert_eq!(compute_delay(0, &cfg, 0.0), Duration::from_millis(1_000));
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_attempt_one_no_jitter() {
        let cfg = AgentRetryConfig::default();
        // 1000 * 2^1 = 2000 ms
        assert_eq!(compute_delay(1, &cfg, 0.0), Duration::from_millis(2_000));
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_attempt_two_no_jitter() {
        let cfg = AgentRetryConfig::default();
        // 1000 * 2^2 = 4000 ms
        assert_eq!(compute_delay(2, &cfg, 0.0), Duration::from_millis(4_000));
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_attempt_three_no_jitter() {
        let cfg = AgentRetryConfig::default();
        // 1000 * 2^3 = 8000 ms
        assert_eq!(compute_delay(3, &cfg, 0.0), Duration::from_millis(8_000));
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_caps_at_max_delay() {
        let cfg = AgentRetryConfig {
            max_retries: 10,
            base_delay_ms: 1_000,
            max_delay_ms: 5_000,
        };
        // 1000 * 2^5 = 32000, capped to 5000
        assert_eq!(compute_delay(5, &cfg, 0.0), Duration::from_millis(5_000));
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_with_jitter_increases_delay() {
        let cfg = AgentRetryConfig::default();
        let no_jitter = compute_delay(1, &cfg, 0.0);
        let with_jitter = compute_delay(1, &cfg, 0.5);
        assert!(with_jitter > no_jitter);
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_with_max_jitter_does_not_exceed_cap() {
        let cfg = AgentRetryConfig {
            max_retries: 10,
            base_delay_ms: 1_000,
            max_delay_ms: 5_000,
        };
        // At attempt 5 the base is already capped at 5000.
        // With 0.5 jitter: 5000 * 1.5 = 7500, but re-capped to 7500
        // (max_delay_ms * 1.5).
        let delay = compute_delay(5, &cfg, 0.5);
        let absolute_cap = Duration::from_millis((cfg.max_delay_ms as f64 * 1.5) as u64);
        assert!(delay <= absolute_cap);
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_with_custom_config_no_jitter() {
        let cfg = AgentRetryConfig {
            max_retries: 5,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
        };
        // 500 * 2^0 = 500
        assert_eq!(compute_delay(0, &cfg, 0.0), Duration::from_millis(500));
        // 500 * 2^1 = 1000
        assert_eq!(compute_delay(1, &cfg, 0.0), Duration::from_millis(1_000));
        // 500 * 2^2 = 2000
        assert_eq!(compute_delay(2, &cfg, 0.0), Duration::from_millis(2_000));
    }

    // @req REQ-AGENT-017
    #[test]
    #[should_panic(expected = "jitter_fraction must be in 0.0..=1.0")]
    fn delay_panics_on_negative_jitter() {
        let cfg = AgentRetryConfig::default();
        compute_delay(0, &cfg, -0.1);
    }

    // @req REQ-AGENT-017
    #[test]
    #[should_panic(expected = "jitter_fraction must be in 0.0..=1.0")]
    fn delay_panics_on_jitter_above_one() {
        let cfg = AgentRetryConfig::default();
        compute_delay(0, &cfg, 1.1);
    }

    // -- RetryOutcome --

    // @req REQ-AGENT-017
    #[test]
    fn retry_outcome_success_holds_value() {
        let outcome: RetryOutcome<u32> = RetryOutcome::Success(42);
        match outcome {
            RetryOutcome::Success(v) => assert_eq!(v, 42),
            RetryOutcome::Exhausted { .. } => {
                panic!("expected Success")
            }
        }
    }

    // @req REQ-AGENT-017
    #[test]
    fn retry_outcome_exhausted_holds_error_and_attempts() {
        let outcome: RetryOutcome<u32> = RetryOutcome::Exhausted {
            last_error: DomainError::ProviderError {
                message: "timeout".into(),
            },
            attempts: 3,
        };
        match outcome {
            RetryOutcome::Exhausted {
                last_error,
                attempts,
            } => {
                assert_eq!(attempts, 3);
                assert!(
                    format!("{last_error}").contains("timeout"),
                    "error should mention timeout"
                );
            }
            RetryOutcome::Success(_) => panic!("expected Exhausted"),
        }
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_jitter_zero_point_five_adds_exactly_fifty_percent() {
        let cfg = AgentRetryConfig {
            max_retries: 3,
            base_delay_ms: 1_000,
            max_delay_ms: 30_000,
        };
        // attempt 0: base = 1000, jittered = 1000 * 1.5 = 1500
        assert_eq!(compute_delay(0, &cfg, 0.5), Duration::from_millis(1_500));
    }

    // @req REQ-AGENT-017
    #[test]
    fn delay_jitter_boundary_one_doubles_base() {
        let cfg = AgentRetryConfig {
            max_retries: 3,
            base_delay_ms: 1_000,
            max_delay_ms: 30_000,
        };
        // attempt 0: base = 1000, jittered = 1000 * 2.0 = 2000
        assert_eq!(compute_delay(0, &cfg, 1.0), Duration::from_millis(2_000));
    }
}
