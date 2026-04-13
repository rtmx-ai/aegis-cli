//! Client-side rate limiting for LLM API calls (REQ-AGENT-018).
//!
//! Implements a sliding-window token bucket that respects provider quotas
//! for both request rate and token throughput. This is complementary to
//! the retry logic in `retry.rs` -- rate limiting prevents hitting limits,
//! while retry handles transient failures after the fact.

use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Rate limiter configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per minute.
    pub requests_per_minute: u32,
    /// Maximum tokens per minute (input + output).
    pub tokens_per_minute: Option<u64>,
    /// Burst allowance (requests above steady rate before throttling).
    pub burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            tokens_per_minute: None,
            burst: 5,
        }
    }
}

/// Token bucket rate limiter for LLM API calls.
pub struct RateLimiter {
    config: RateLimitConfig,
    /// Timestamps of recent requests (sliding window).
    request_times: Mutex<Vec<Instant>>,
    /// Token count in current window.
    tokens_used: Mutex<u64>,
    /// Window start time for token counting.
    window_start: Mutex<Instant>,
}

/// Duration of the sliding window.
const WINDOW: Duration = Duration::from_secs(60);

impl RateLimiter {
    /// Create a new rate limiter with the given configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            request_times: Mutex::new(Vec::new()),
            tokens_used: Mutex::new(0),
            window_start: Mutex::new(Instant::now()),
        }
    }

    /// Wait until a request is allowed under the rate limit.
    /// Returns the wait duration (zero if no wait needed).
    pub async fn acquire(&self) -> Duration {
        let effective_limit = self.config.requests_per_minute + self.config.burst;

        loop {
            let now = Instant::now();
            let mut times = self.request_times.lock().await;
            times.retain(|t| now.duration_since(*t) < WINDOW);

            if (times.len() as u32) < effective_limit {
                // Check token limit if configured.
                if let Some(token_limit) = self.config.tokens_per_minute {
                    let mut tokens = self.tokens_used.lock().await;
                    let mut ws = self.window_start.lock().await;
                    if now.duration_since(*ws) >= WINDOW {
                        *tokens = 0;
                        *ws = now;
                    }
                    if *tokens >= token_limit {
                        let wait = WINDOW
                            .checked_sub(now.duration_since(*ws))
                            .unwrap_or(Duration::ZERO);
                        drop(tokens);
                        drop(ws);
                        drop(times);
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }
                times.push(now);
                // Calculate how long we actually waited (zero on first pass).
                return Duration::ZERO;
            }

            // Find oldest timestamp and wait until it expires from the window.
            let oldest = times[0];
            let wait = WINDOW
                .checked_sub(now.duration_since(oldest))
                .unwrap_or(Duration::ZERO);
            drop(times);
            tokio::time::sleep(wait).await;

            // Re-acquire after sleep; record the timestamp on the next iteration.
            let now_after = Instant::now();
            let mut times = self.request_times.lock().await;
            times.retain(|t| now_after.duration_since(*t) < WINDOW);
            if (times.len() as u32) < effective_limit {
                times.push(now_after);
                return wait;
            }
            // Rare race: loop again.
        }
    }

    /// Record token usage from a completed request.
    pub async fn record_usage(&self, input_tokens: u64, output_tokens: u64) {
        let now = Instant::now();
        let mut tokens = self.tokens_used.lock().await;
        let mut ws = self.window_start.lock().await;
        if now.duration_since(*ws) >= WINDOW {
            *tokens = 0;
            *ws = now;
        }
        *tokens += input_tokens + output_tokens;
    }

    /// Check if currently rate limited (without waiting).
    pub async fn is_limited(&self) -> bool {
        let effective_limit = self.config.requests_per_minute + self.config.burst;
        let now = Instant::now();
        let mut times = self.request_times.lock().await;
        times.retain(|t| now.duration_since(*t) < WINDOW);
        (times.len() as u32) >= effective_limit
    }

    /// Get current utilization as a percentage (0.0 to 1.0).
    /// Based on requests_per_minute (not including burst).
    pub async fn utilization(&self) -> f64 {
        let now = Instant::now();
        let mut times = self.request_times.lock().await;
        times.retain(|t| now.duration_since(*t) < WINDOW);
        let count = times.len() as f64;
        let limit = self.config.requests_per_minute as f64;
        if limit == 0.0 {
            return 1.0;
        }
        (count / limit).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-AGENT-018
    #[test]
    fn default_config_values() {
        let cfg = RateLimitConfig::default();
        assert_eq!(cfg.requests_per_minute, 60);
        assert_eq!(cfg.burst, 5);
        assert!(cfg.tokens_per_minute.is_none());
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test]
    async fn acquire_returns_zero_when_not_limited() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        let wait = limiter.acquire().await;
        assert_eq!(wait, Duration::ZERO);
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test(start_paused = true)]
    async fn acquire_waits_when_at_limit() {
        let config = RateLimitConfig {
            requests_per_minute: 2,
            tokens_per_minute: None,
            burst: 0,
        };
        let limiter = RateLimiter::new(config);

        // Use up the limit.
        limiter.acquire().await;
        limiter.acquire().await;

        // Third should require a wait.
        let wait = limiter.acquire().await;
        assert!(
            wait > Duration::ZERO,
            "expected nonzero wait, got {:?}",
            wait
        );
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test]
    async fn record_usage_tracks_tokens() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            tokens_per_minute: Some(10_000),
            burst: 0,
        };
        let limiter = RateLimiter::new(config);
        limiter.record_usage(500, 200).await;
        limiter.record_usage(300, 100).await;

        let tokens = *limiter.tokens_used.lock().await;
        assert_eq!(tokens, 1_100);
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test]
    async fn is_limited_returns_false_initially() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        assert!(!limiter.is_limited().await);
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test]
    async fn is_limited_returns_true_at_capacity() {
        let config = RateLimitConfig {
            requests_per_minute: 3,
            tokens_per_minute: None,
            burst: 0,
        };
        let limiter = RateLimiter::new(config);
        limiter.acquire().await;
        limiter.acquire().await;
        limiter.acquire().await;
        assert!(limiter.is_limited().await);
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test]
    async fn utilization_increases_with_requests() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            tokens_per_minute: None,
            burst: 5,
        };
        let limiter = RateLimiter::new(config);

        // 0 of 60 = 0.0
        assert!((limiter.utilization().await - 0.0).abs() < f64::EPSILON);

        // Fire 30 requests.
        for _ in 0..30 {
            limiter.acquire().await;
        }

        let util = limiter.utilization().await;
        assert!(
            (util - 0.5).abs() < f64::EPSILON,
            "expected ~0.5, got {util}"
        );
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test(start_paused = true)]
    async fn window_slides_after_60_seconds() {
        let config = RateLimitConfig {
            requests_per_minute: 2,
            tokens_per_minute: None,
            burst: 0,
        };
        let limiter = RateLimiter::new(config);

        limiter.acquire().await;
        limiter.acquire().await;
        assert!(limiter.is_limited().await);

        // Advance time past the window.
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(!limiter.is_limited().await);
    }

    // rtmx:req REQ-AGENT-018
    #[tokio::test]
    async fn burst_allows_temporary_overshoot() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            tokens_per_minute: None,
            burst: 5,
        };
        let limiter = RateLimiter::new(config);

        // Should be able to fire 65 requests (60 + 5 burst) without blocking.
        for _ in 0..65 {
            let wait = limiter.acquire().await;
            assert_eq!(wait, Duration::ZERO, "request within burst should not wait");
        }

        // The 66th should be limited.
        assert!(limiter.is_limited().await);
    }
}
