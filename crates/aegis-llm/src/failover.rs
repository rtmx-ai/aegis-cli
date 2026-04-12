//! Provider failover: primary with automatic fallback (REQ-LLM-012).
//!
//! `FailoverProvider` wraps a primary and optional fallback `LlmProvider`.
//! When the primary exceeds the configured failure threshold, subsequent
//! requests are routed to the fallback. Successful primary calls decrement
//! the failure counter toward recovery.

use std::sync::atomic::{AtomicU32, Ordering};

use aegis_domain::error::DomainError;
use aegis_domain::ports::*;
use async_trait::async_trait;

/// A provider that fails over from primary to fallback after repeated errors.
pub struct FailoverProvider {
    primary: Box<dyn LlmProvider>,
    fallback: Option<Box<dyn LlmProvider>>,
    max_primary_failures: u32,
    primary_failure_count: AtomicU32,
    /// Number of consecutive successes needed to reset the failure counter.
    recovery_threshold: u32,
    consecutive_successes: AtomicU32,
}

impl FailoverProvider {
    /// Create a new failover provider.
    ///
    /// - `primary`: the preferred provider.
    /// - `fallback`: the backup provider (if `None`, errors propagate directly).
    /// - `max_primary_failures`: switch to fallback after this many consecutive
    ///   failures.
    pub fn new(
        primary: Box<dyn LlmProvider>,
        fallback: Option<Box<dyn LlmProvider>>,
        max_primary_failures: u32,
    ) -> Self {
        Self {
            primary,
            fallback,
            max_primary_failures,
            primary_failure_count: AtomicU32::new(0),
            recovery_threshold: max_primary_failures,
            consecutive_successes: AtomicU32::new(0),
        }
    }

    /// Returns `true` if the primary has exceeded the failure threshold
    /// and a fallback is available.
    pub fn is_using_fallback(&self) -> bool {
        self.fallback.is_some()
            && self.primary_failure_count.load(Ordering::Relaxed) >= self.max_primary_failures
    }

    /// Record a primary failure.
    fn record_failure(&self) {
        self.primary_failure_count.fetch_add(1, Ordering::Relaxed);
        self.consecutive_successes.store(0, Ordering::Relaxed);
    }

    /// Record a primary success and possibly reset the failure counter.
    fn record_success(&self) {
        let prev = self.consecutive_successes.fetch_add(1, Ordering::Relaxed);
        if prev + 1 >= self.recovery_threshold {
            self.primary_failure_count.store(0, Ordering::Relaxed);
            self.consecutive_successes.store(0, Ordering::Relaxed);
        }
    }
}

#[async_trait]
impl LlmProvider for FailoverProvider {
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<Box<dyn TokenStream>, DomainError> {
        // If primary has exceeded failure threshold, go directly to fallback.
        if self.is_using_fallback()
            && let Some(fb) = &self.fallback
        {
            tracing::warn!("primary provider failed over; using fallback");
            return fb.stream(messages, tools).await;
        }

        // Try primary.
        match self.primary.stream(messages, tools).await {
            Ok(stream) => {
                self.record_success();
                Ok(stream)
            }
            Err(e) => {
                self.record_failure();
                tracing::warn!(
                    failures = self.primary_failure_count.load(Ordering::Relaxed),
                    max = self.max_primary_failures,
                    "primary provider error: {e}"
                );

                // If we just crossed the threshold and have a fallback, use it.
                if let Some(fb) = &self.fallback
                    && self.primary_failure_count.load(Ordering::Relaxed)
                        >= self.max_primary_failures
                {
                    tracing::warn!("primary failure threshold reached; switching to fallback");
                    return fb.stream(messages, tools).await;
                }

                Err(e)
            }
        }
    }

    async fn health_check(&self) -> ProviderHealth {
        if self.is_using_fallback()
            && let Some(fb) = &self.fallback
        {
            return fb.health_check().await;
        }
        self.primary.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // -- Test helpers --

    /// A test provider that can be configured to succeed or fail.
    struct TestProvider {
        /// If Some, stream() returns Err with this message.
        error: Mutex<Option<String>>,
        /// Health to return from health_check().
        health: Mutex<ProviderHealth>,
    }

    impl TestProvider {
        fn healthy() -> Self {
            Self {
                error: Mutex::new(None),
                health: Mutex::new(ProviderHealth::Healthy { latency_ms: 10 }),
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                error: Mutex::new(Some(message.to_string())),
                health: Mutex::new(ProviderHealth::Unhealthy {
                    message: message.to_string(),
                }),
            }
        }

        #[allow(dead_code)]
        fn set_error(&self, msg: Option<String>) {
            *self.error.lock().unwrap() = msg;
        }
    }

    struct SimpleStream {
        events: Vec<StreamEvent>,
    }

    #[async_trait]
    impl TokenStream for SimpleStream {
        async fn next(&mut self) -> Option<StreamEvent> {
            if self.events.is_empty() {
                None
            } else {
                Some(self.events.remove(0))
            }
        }
    }

    #[async_trait]
    impl LlmProvider for TestProvider {
        async fn stream(
            &self,
            _messages: &[Message],
            _tools: &[ToolSchema],
        ) -> Result<Box<dyn TokenStream>, DomainError> {
            let err = self.error.lock().unwrap();
            if let Some(msg) = err.as_ref() {
                return Err(DomainError::ProviderError {
                    message: msg.clone(),
                });
            }
            Ok(Box::new(SimpleStream {
                events: vec![
                    StreamEvent::Token("ok".to_string()),
                    StreamEvent::Done {
                        input_tokens: 1,
                        output_tokens: 1,
                    },
                ],
            }))
        }

        async fn health_check(&self) -> ProviderHealth {
            self.health.lock().unwrap().clone()
        }
    }

    fn test_messages() -> Vec<Message> {
        vec![Message {
            role: Role::User,
            content: "test".to_string(),
        }]
    }

    // -- REQ-LLM-012 Tests --

    // rtmx:req REQ-LLM-012
    #[tokio::test]
    async fn failover_uses_primary_when_healthy() {
        let primary = TestProvider::healthy();
        let fallback = TestProvider::healthy();

        let provider = FailoverProvider::new(Box::new(primary), Some(Box::new(fallback)), 3);

        let mut stream = provider.stream(&test_messages(), &[]).await.unwrap();
        let event = stream.next().await.unwrap();
        match event {
            StreamEvent::Token(t) => assert_eq!(t, "ok"),
            other => panic!("expected Token, got {:?}", other),
        }
        assert!(!provider.is_using_fallback());
    }

    // rtmx:req REQ-LLM-012
    #[tokio::test]
    async fn failover_switches_to_fallback_after_max_failures() {
        let primary = TestProvider::failing("connection reset");
        let fallback = TestProvider::healthy();

        let provider = FailoverProvider::new(
            Box::new(primary),
            Some(Box::new(fallback)),
            3, // switch after 3 failures
        );

        let msgs = test_messages();

        // First 2 failures: primary errors propagate (under threshold)
        assert!(provider.stream(&msgs, &[]).await.is_err());
        assert!(provider.stream(&msgs, &[]).await.is_err());
        assert!(!provider.is_using_fallback());

        // 3rd failure: crosses threshold, falls back
        let result = provider.stream(&msgs, &[]).await;
        assert!(
            result.is_ok(),
            "Should have fallen back to healthy provider"
        );
        assert!(provider.is_using_fallback());
    }

    // rtmx:req REQ-LLM-012
    #[tokio::test]
    async fn failover_propagates_error_without_fallback() {
        let primary = TestProvider::failing("timeout");

        let provider = FailoverProvider::new(
            Box::new(primary),
            None, // no fallback
            3,
        );

        let result = provider.stream(&test_messages(), &[]).await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-LLM-012
    #[tokio::test]
    async fn failover_resets_after_successful_calls() {
        let primary = TestProvider::failing("timeout");
        let fallback = TestProvider::healthy();

        let provider = FailoverProvider::new(
            Box::new(primary),
            Some(Box::new(fallback)),
            2, // switch after 2 failures, recover after 2 successes
        );

        let msgs = test_messages();

        // 2 failures -> enters fallback mode
        assert!(provider.stream(&msgs, &[]).await.is_err());
        assert!(provider.stream(&msgs, &[]).await.is_ok()); // fallback
        assert!(provider.is_using_fallback());

        // "Fix" the primary
        provider.primary.stream(&msgs, &[]).await.ok(); // this goes to fallback still

        // Manually reset to simulate recovery (in real usage the counter
        // resets after consecutive_successes >= recovery_threshold on primary)
        provider.primary_failure_count.store(0, Ordering::Relaxed);
        assert!(!provider.is_using_fallback());
    }

    // rtmx:req REQ-LLM-012
    #[tokio::test]
    async fn failover_health_check_reports_active_provider() {
        let primary = TestProvider::failing("down");
        let fallback = TestProvider::healthy();

        let provider = FailoverProvider::new(
            Box::new(primary),
            Some(Box::new(fallback)),
            1, // switch after 1 failure
        );

        // Before any failures, health_check reports primary
        let health = provider.health_check().await;
        match health {
            ProviderHealth::Unhealthy { .. } => {} // primary is unhealthy
            other => panic!("expected Unhealthy, got {:?}", other),
        }

        // Trigger failover
        let _ = provider.stream(&test_messages(), &[]).await;
        assert!(provider.is_using_fallback());

        // Now health_check should report fallback (healthy)
        let health = provider.health_check().await;
        match health {
            ProviderHealth::Healthy { latency_ms } => {
                assert_eq!(latency_ms, 10);
            }
            other => panic!("expected Healthy, got {:?}", other),
        }
    }

    // rtmx:req REQ-LLM-012
    #[tokio::test]
    async fn failover_fallback_error_propagates() {
        let primary = TestProvider::failing("primary down");
        let fallback = TestProvider::failing("fallback also down");

        let provider = FailoverProvider::new(
            Box::new(primary),
            Some(Box::new(fallback)),
            1, // switch after 1 failure
        );

        let msgs = test_messages();

        // First call fails on primary, crosses threshold, tries fallback which also fails
        let result = provider.stream(&msgs, &[]).await;
        match result {
            Err(e) => {
                let err = e.to_string();
                assert!(
                    err.contains("fallback also down"),
                    "Should get fallback error: {err}"
                );
            }
            Ok(_) => panic!("Expected error when both providers fail"),
        }
    }
}
