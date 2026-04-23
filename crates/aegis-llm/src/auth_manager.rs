//! Shared credential store with lifecycle tracking.
//!
//! `AuthManager` provides a thread-safe cache of resolved `ProviderAuth`
//! credentials, keyed by `ProviderKind`. It tracks expiry, emits status
//! events for the TUI, and delegates to `resolve_auth()` when a credential
//! is missing or expired.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::auth::{ProviderAuth, resolve_auth};
use crate::config::{ProviderConfig, ProviderKind};
use aegis_domain::error::DomainError;

/// Events emitted by `AuthManager` to notify the TUI of credential state changes.
#[derive(Debug, Clone)]
pub enum AuthStatusEvent {
    /// Token resolved successfully.
    Authenticated {
        provider: ProviderKind,
        ttl_secs: u64,
    },
    /// Token expiring soon (< 120s).
    ExpiryWarning {
        provider: ProviderKind,
        remaining_secs: u64,
    },
    /// Token has expired.
    Expired { provider: ProviderKind },
    /// Device code flow started -- TUI should display URL and code.
    DeviceCodePending {
        provider: ProviderKind,
        url: String,
        user_code: String,
    },
    /// Device code approved -- token received.
    DeviceCodeComplete { provider: ProviderKind },
    /// Refresh attempt failed.
    RefreshFailed {
        provider: ProviderKind,
        reason: String,
    },
}

/// Represents a pending device code flow.
#[derive(Debug, Clone)]
pub struct DeviceCodeFlow {
    pub provider: ProviderKind,
    pub verification_url: String,
    pub user_code: String,
    pub device_code: String,
    pub poll_interval_secs: u64,
    pub expires_at: Instant,
}

/// Cached credential state for a single provider.
#[derive(Debug, Clone)]
pub struct AuthState {
    pub auth: ProviderAuth,
    pub expires_at: Option<Instant>,
    pub refresh_token: Option<String>,
    pub provider_kind: ProviderKind,
}

/// Thread-safe credential store with lifecycle tracking.
pub struct AuthManager {
    credentials: RwLock<HashMap<ProviderKind, AuthState>>,
    status_tx: mpsc::UnboundedSender<AuthStatusEvent>,
}

/// Return the default TTL in seconds for a given provider kind.
///
/// Local providers have no expiry. GCP and Azure tokens expire in 1 hour.
/// AWS STS temporary credentials last up to 12 hours.
fn default_ttl_secs(kind: ProviderKind) -> Option<u64> {
    match kind {
        ProviderKind::Local => None,
        ProviderKind::Vertex => Some(3600),
        ProviderKind::Bedrock => Some(43200),
        ProviderKind::Azure => Some(3600),
    }
}

impl AuthManager {
    /// Construct an `AuthManager` with an empty credential cache.
    ///
    /// The `status_tx` channel is used to emit `AuthStatusEvent` notifications
    /// to the TUI or other observers.
    pub fn new(status_tx: mpsc::UnboundedSender<AuthStatusEvent>) -> Self {
        Self {
            credentials: RwLock::new(HashMap::new()),
            status_tx,
        }
    }

    /// Resolve a credential for the given provider config, using the cache
    /// if a valid (non-expired) credential is available.
    ///
    /// On cache miss or expiry, delegates to `resolve_auth()` from the auth
    /// module, caches the result with a provider-appropriate TTL, and emits
    /// an `AuthStatusEvent::Authenticated` event.
    pub fn resolve_or_refresh(
        &self,
        config: &ProviderConfig,
    ) -> Result<ProviderAuth, DomainError> {
        // Fast path: check if we have a valid cached credential.
        {
            let creds = self
                .credentials
                .read()
                .map_err(|e| DomainError::ProviderError {
                    message: format!("credential cache lock poisoned: {e}"),
                })?;
            if let Some(state) = creds.get(&config.kind)
                && self.is_state_valid(state)
            {
                return Ok(state.auth.clone());
            }
        }

        // Slow path: resolve fresh credentials.
        let auth = resolve_auth(config)?;
        let ttl = default_ttl_secs(config.kind);
        let expires_at = ttl.map(|secs| Instant::now() + Duration::from_secs(secs));

        let state = AuthState {
            auth: auth.clone(),
            expires_at,
            refresh_token: None,
            provider_kind: config.kind,
        };

        {
            let mut creds = self
                .credentials
                .write()
                .map_err(|e| DomainError::ProviderError {
                    message: format!("credential cache lock poisoned: {e}"),
                })?;
            creds.insert(config.kind, state);
        }

        // Best-effort event emission; receiver may have been dropped.
        let _ = self.status_tx.send(AuthStatusEvent::Authenticated {
            provider: config.kind,
            ttl_secs: ttl.unwrap_or(0),
        });

        Ok(auth)
    }

    /// Check whether a cached credential exists and has not expired.
    pub fn is_valid(&self, kind: ProviderKind) -> bool {
        let creds = match self.credentials.read() {
            Ok(c) => c,
            Err(_) => return false,
        };
        match creds.get(&kind) {
            Some(state) => self.is_state_valid(state),
            None => false,
        }
    }

    /// Return the remaining time-to-live for a cached credential.
    ///
    /// Returns `None` if no credential is cached, or if the credential
    /// has no expiry (e.g. local providers).
    pub fn ttl(&self, kind: ProviderKind) -> Option<Duration> {
        let creds = self.credentials.read().ok()?;
        let state = creds.get(&kind)?;
        let expires_at = state.expires_at?;
        let now = Instant::now();
        if expires_at > now {
            Some(expires_at - now)
        } else {
            Some(Duration::ZERO)
        }
    }

    /// Remove a cached credential. Useful when switching providers via
    /// the `/connect` slash command.
    pub fn revoke(&self, kind: ProviderKind) {
        if let Ok(mut creds) = self.credentials.write() {
            creds.remove(&kind);
        }
    }

    /// Directly cache a credential with an explicit TTL and optional
    /// refresh token. Used by the device code flow and tests.
    pub fn cache_auth(
        &self,
        kind: ProviderKind,
        auth: ProviderAuth,
        ttl_secs: Option<u64>,
        refresh_token: Option<String>,
    ) {
        let expires_at = ttl_secs.map(|secs| Instant::now() + Duration::from_secs(secs));
        let state = AuthState {
            auth,
            expires_at,
            refresh_token,
            provider_kind: kind,
        };
        if let Ok(mut creds) = self.credentials.write() {
            creds.insert(kind, state);
        }
    }

    /// Initiate a device code flow for the given provider.
    /// Emits `DeviceCodePending` event with URL and code for TUI display.
    /// Returns the `DeviceCodeFlow` for polling.
    ///
    /// For now, this is a structured placeholder that:
    /// 1. Constructs the correct OAuth endpoint URL per provider
    /// 2. Emits the `DeviceCodePending` event
    /// 3. Returns a `DeviceCodeFlow` struct
    ///
    /// Actual HTTP requests to OAuth endpoints will be wired in
    /// the composition root when reqwest is available.
    pub fn initiate_device_code(
        &self,
        provider: ProviderKind,
    ) -> Result<DeviceCodeFlow, DomainError> {
        let (verification_url, poll_interval) = match provider {
            ProviderKind::Vertex => {
                ("https://accounts.google.com/o/oauth2/device".to_string(), 5)
            }
            ProviderKind::Bedrock => (
                "https://device.sso.us-gov-west-1.amazonaws.com/".to_string(),
                5,
            ),
            ProviderKind::Azure => (
                "https://login.microsoftonline.com/common/oauth2/v2.0/devicecode".to_string(),
                5,
            ),
            ProviderKind::Local => {
                return Err(DomainError::ConfigError {
                    message: "device code flow not applicable for local provider".into(),
                });
            }
        };

        // Generate placeholder codes (real implementation will call OAuth endpoint)
        let flow = DeviceCodeFlow {
            provider,
            verification_url: verification_url.clone(),
            user_code: "PENDING".to_string(),
            device_code: String::new(),
            poll_interval_secs: poll_interval,
            expires_at: Instant::now() + Duration::from_secs(300),
        };

        // Emit event for TUI display
        let _ = self.status_tx.send(AuthStatusEvent::DeviceCodePending {
            provider,
            url: verification_url,
            user_code: flow.user_code.clone(),
        });

        Ok(flow)
    }

    /// Complete a device code flow by caching the resolved credentials.
    /// Called by the composition root after polling succeeds.
    pub fn complete_device_code(
        &self,
        flow: &DeviceCodeFlow,
        auth: ProviderAuth,
        ttl_secs: u64,
        refresh_token: Option<String>,
    ) {
        self.cache_auth(flow.provider, auth, Some(ttl_secs), refresh_token);
        let _ = self.status_tx.send(AuthStatusEvent::DeviceCodeComplete {
            provider: flow.provider,
        });
    }

    /// Signal that a device code flow has timed out or failed.
    pub fn fail_device_code(&self, provider: ProviderKind, reason: String) {
        let _ = self
            .status_tx
            .send(AuthStatusEvent::RefreshFailed { provider, reason });
    }

    /// Return providers whose cached credentials are expired or near-expiry
    /// and have a refresh token available.
    ///
    /// For each cached credential:
    /// - If expired (remaining = 0): emits `Expired`, included in return vec.
    /// - If near-expiry (remaining < threshold) AND has refresh_token: emits
    ///   `ExpiryWarning`, included in return vec.
    /// - If near-expiry but no refresh_token: emits `ExpiryWarning` only
    ///   (not included in return vec).
    /// - Credentials with no expiry (e.g. Local) are skipped entirely.
    pub fn check_expiry(&self, refresh_threshold_secs: u64) -> Vec<ProviderKind> {
        let creds = match self.credentials.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut needs_refresh = Vec::new();
        let now = Instant::now();

        for (kind, state) in creds.iter() {
            let expires_at = match state.expires_at {
                Some(ea) => ea,
                None => continue, // no expiry (e.g. Local)
            };

            let remaining = if expires_at > now {
                (expires_at - now).as_secs()
            } else {
                0
            };

            if remaining == 0 {
                let _ = self
                    .status_tx
                    .send(AuthStatusEvent::Expired { provider: *kind });
                needs_refresh.push(*kind);
            } else if remaining < refresh_threshold_secs {
                let _ = self.status_tx.send(AuthStatusEvent::ExpiryWarning {
                    provider: *kind,
                    remaining_secs: remaining,
                });
                if state.refresh_token.is_some() {
                    needs_refresh.push(*kind);
                }
            }
        }

        needs_refresh
    }

    /// Check whether a cached credential has a refresh token.
    pub fn has_refresh_token(&self, kind: ProviderKind) -> bool {
        let creds = match self.credentials.read() {
            Ok(c) => c,
            Err(_) => return false,
        };
        matches!(creds.get(&kind), Some(state) if state.refresh_token.is_some())
    }

    /// Return the refresh token for a cached credential, if present.
    pub fn get_refresh_token(&self, kind: ProviderKind) -> Option<String> {
        let creds = self.credentials.read().ok()?;
        creds.get(&kind)?.refresh_token.clone()
    }

    /// Check whether an `AuthState` is still valid (not expired).
    fn is_state_valid(&self, state: &AuthState) -> bool {
        match state.expires_at {
            None => true,
            Some(expires_at) => Instant::now() < expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> (AuthManager, mpsc::UnboundedReceiver<AuthStatusEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (AuthManager::new(tx), rx)
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_resolve_caches_credential() {
        let (mgr, _rx) = make_manager();
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");

        // First resolve populates the cache.
        let auth1 = mgr.resolve_or_refresh(&cfg).unwrap();
        assert_eq!(auth1, ProviderAuth::NoAuth);
        assert!(mgr.is_valid(ProviderKind::Local));

        // Second resolve returns the cached credential.
        let auth2 = mgr.resolve_or_refresh(&cfg).unwrap();
        assert_eq!(auth2, ProviderAuth::NoAuth);
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_expired_credential_triggers_re_resolve() {
        let (mgr, _rx) = make_manager();

        // Directly cache a credential that is already expired.
        let expired_at = Instant::now() - Duration::from_secs(1);
        {
            let mut creds = mgr.credentials.write().unwrap();
            creds.insert(
                ProviderKind::Local,
                AuthState {
                    auth: ProviderAuth::NoAuth,
                    expires_at: Some(expired_at),
                    refresh_token: None,
                    provider_kind: ProviderKind::Local,
                },
            );
        }

        // The expired credential should not be considered valid.
        assert!(!mgr.is_valid(ProviderKind::Local));

        // resolve_or_refresh should call resolve_auth again.
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");
        let auth = mgr.resolve_or_refresh(&cfg).unwrap();
        assert_eq!(auth, ProviderAuth::NoAuth);

        // Now it should be valid again (re-cached without expiry for Local).
        assert!(mgr.is_valid(ProviderKind::Local));
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_is_valid_returns_false_for_missing() {
        let (mgr, _rx) = make_manager();
        assert!(!mgr.is_valid(ProviderKind::Vertex));
        assert!(!mgr.is_valid(ProviderKind::Bedrock));
        assert!(!mgr.is_valid(ProviderKind::Azure));
        assert!(!mgr.is_valid(ProviderKind::Local));
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_ttl_returns_remaining_duration() {
        let (mgr, _rx) = make_manager();

        // Cache a Vertex credential with 3600s TTL.
        mgr.cache_auth(
            ProviderKind::Vertex,
            ProviderAuth::Gcp {
                access_token: "ya29.test".to_string(),
            },
            Some(3600),
            None,
        );

        let ttl = mgr.ttl(ProviderKind::Vertex).unwrap();
        // Should be close to 3600s (allow 5s tolerance for test execution).
        assert!(ttl.as_secs() >= 3595);
        assert!(ttl.as_secs() <= 3600);
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_revoke_clears_cached_credential() {
        let (mgr, _rx) = make_manager();

        mgr.cache_auth(
            ProviderKind::Vertex,
            ProviderAuth::Gcp {
                access_token: "ya29.test".to_string(),
            },
            Some(3600),
            None,
        );
        assert!(mgr.is_valid(ProviderKind::Vertex));

        mgr.revoke(ProviderKind::Vertex);
        assert!(!mgr.is_valid(ProviderKind::Vertex));
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_authenticated_event_emitted_on_resolve() {
        let (mgr, mut rx) = make_manager();
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");

        mgr.resolve_or_refresh(&cfg).unwrap();

        let event = rx.try_recv().unwrap();
        match event {
            AuthStatusEvent::Authenticated { provider, ttl_secs } => {
                assert_eq!(provider, ProviderKind::Local);
                // Local providers have no TTL, so ttl_secs should be 0.
                assert_eq!(ttl_secs, 0);
            }
            other => panic!("expected Authenticated event, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_local_provider_has_no_expiry() {
        let (mgr, _rx) = make_manager();
        let cfg = ProviderConfig::local("http://localhost:11434/v1", "llama3");

        mgr.resolve_or_refresh(&cfg).unwrap();

        // Local provider should have no TTL (None).
        assert!(mgr.ttl(ProviderKind::Local).is_none());
        // But it should still be valid.
        assert!(mgr.is_valid(ProviderKind::Local));
    }

    // rtmx:req REQ-LLM-034
    #[test]
    fn test_multiple_providers_cached_independently() {
        let (mgr, _rx) = make_manager();

        let vertex_auth = ProviderAuth::Gcp {
            access_token: "ya29.vertex-token".to_string(),
        };
        let bedrock_auth = ProviderAuth::Aws {
            access_key_id: "AKIA_TEST".to_string(),
            secret_access_key: "secret_test".to_string(),
            session_token: None,
            region: "us-east-1".to_string(),
        };

        mgr.cache_auth(ProviderKind::Vertex, vertex_auth.clone(), Some(3600), None);
        mgr.cache_auth(
            ProviderKind::Bedrock,
            bedrock_auth.clone(),
            Some(43200),
            None,
        );

        assert!(mgr.is_valid(ProviderKind::Vertex));
        assert!(mgr.is_valid(ProviderKind::Bedrock));

        // Verify each returns its own auth by checking TTL differences.
        let vertex_ttl = mgr.ttl(ProviderKind::Vertex).unwrap();
        let bedrock_ttl = mgr.ttl(ProviderKind::Bedrock).unwrap();
        assert!(vertex_ttl.as_secs() <= 3600);
        assert!(bedrock_ttl.as_secs() > 3600);

        // Revoking one does not affect the other.
        mgr.revoke(ProviderKind::Vertex);
        assert!(!mgr.is_valid(ProviderKind::Vertex));
        assert!(mgr.is_valid(ProviderKind::Bedrock));
    }

    // --- Device code flow tests ---

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_initiate_device_code_vertex() {
        let (mgr, mut rx) = make_manager();
        let flow = mgr.initiate_device_code(ProviderKind::Vertex).unwrap();
        assert!(flow.verification_url.contains("google"));
        assert_eq!(flow.provider, ProviderKind::Vertex);

        let event = rx.try_recv().unwrap();
        match event {
            AuthStatusEvent::DeviceCodePending { provider, url, .. } => {
                assert_eq!(provider, ProviderKind::Vertex);
                assert!(url.contains("google"));
            }
            other => panic!("expected DeviceCodePending, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_initiate_device_code_bedrock() {
        let (mgr, _rx) = make_manager();
        let flow = mgr.initiate_device_code(ProviderKind::Bedrock).unwrap();
        assert!(flow.verification_url.contains("sso"));
        assert_eq!(flow.provider, ProviderKind::Bedrock);
    }

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_initiate_device_code_azure() {
        let (mgr, _rx) = make_manager();
        let flow = mgr.initiate_device_code(ProviderKind::Azure).unwrap();
        assert!(flow.verification_url.contains("microsoft"));
        assert_eq!(flow.provider, ProviderKind::Azure);
    }

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_initiate_device_code_local_errors() {
        let (mgr, _rx) = make_manager();
        let result = mgr.initiate_device_code(ProviderKind::Local);
        assert!(result.is_err());
        match result.unwrap_err() {
            DomainError::ConfigError { message } => {
                assert!(message.contains("local provider"));
            }
            other => panic!("expected ConfigError, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_complete_device_code_caches_auth() {
        let (mgr, _rx) = make_manager();
        let flow = mgr.initiate_device_code(ProviderKind::Vertex).unwrap();

        let auth = ProviderAuth::Gcp {
            access_token: "ya29.device-code-token".to_string(),
        };
        mgr.complete_device_code(&flow, auth, 3600, None);

        assert!(mgr.is_valid(ProviderKind::Vertex));
    }

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_complete_device_code_emits_event() {
        let (mgr, mut rx) = make_manager();
        let flow = mgr.initiate_device_code(ProviderKind::Vertex).unwrap();

        // Drain the DeviceCodePending event from initiation.
        let _ = rx.try_recv().unwrap();

        let auth = ProviderAuth::Gcp {
            access_token: "ya29.device-code-token".to_string(),
        };
        mgr.complete_device_code(&flow, auth, 3600, None);

        let event = rx.try_recv().unwrap();
        match event {
            AuthStatusEvent::DeviceCodeComplete { provider } => {
                assert_eq!(provider, ProviderKind::Vertex);
            }
            other => panic!("expected DeviceCodeComplete, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_fail_device_code_emits_event() {
        let (mgr, mut rx) = make_manager();
        mgr.fail_device_code(
            ProviderKind::Bedrock,
            "authorization_pending timeout".to_string(),
        );

        let event = rx.try_recv().unwrap();
        match event {
            AuthStatusEvent::RefreshFailed { provider, reason } => {
                assert_eq!(provider, ProviderKind::Bedrock);
                assert!(reason.contains("timeout"));
            }
            other => panic!("expected RefreshFailed, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-035
    #[test]
    fn test_device_code_flow_has_5min_expiry() {
        let (mgr, _rx) = make_manager();
        let before = Instant::now() + Duration::from_secs(295);
        let flow = mgr.initiate_device_code(ProviderKind::Vertex).unwrap();
        let after = Instant::now() + Duration::from_secs(305);

        // expires_at should be approximately 300s from now.
        assert!(
            flow.expires_at >= before,
            "expires_at should be at least ~295s from now"
        );
        assert!(
            flow.expires_at <= after,
            "expires_at should be at most ~305s from now"
        );
    }

    // --- Token refresh check tests ---

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_check_expiry_returns_empty_when_all_valid() {
        let (mgr, _rx) = make_manager();
        mgr.cache_auth(
            ProviderKind::Vertex,
            ProviderAuth::Gcp {
                access_token: "ya29.valid".to_string(),
            },
            Some(3600),
            Some("refresh-tok".to_string()),
        );

        let result = mgr.check_expiry(120);
        assert!(result.is_empty());
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_check_expiry_returns_provider_when_near_expiry() {
        let (mgr, _rx) = make_manager();

        // Cache a credential that expires in 60s (below 120s threshold).
        mgr.cache_auth(
            ProviderKind::Vertex,
            ProviderAuth::Gcp {
                access_token: "ya29.expiring".to_string(),
            },
            Some(60),
            Some("refresh-tok".to_string()),
        );

        let result = mgr.check_expiry(120);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ProviderKind::Vertex);
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_check_expiry_emits_warning_event() {
        let (mgr, mut rx) = make_manager();

        mgr.cache_auth(
            ProviderKind::Vertex,
            ProviderAuth::Gcp {
                access_token: "ya29.expiring".to_string(),
            },
            Some(60),
            Some("refresh-tok".to_string()),
        );

        let _ = mgr.check_expiry(120);

        let event = rx.try_recv().unwrap();
        match event {
            AuthStatusEvent::ExpiryWarning {
                provider,
                remaining_secs,
            } => {
                assert_eq!(provider, ProviderKind::Vertex);
                assert!(remaining_secs <= 60);
            }
            other => panic!("expected ExpiryWarning, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_check_expiry_emits_expired_event() {
        let (mgr, mut rx) = make_manager();

        // Cache an already-expired credential.
        {
            let mut creds = mgr.credentials.write().unwrap();
            creds.insert(
                ProviderKind::Bedrock,
                AuthState {
                    auth: ProviderAuth::Aws {
                        access_key_id: "AKIA".to_string(),
                        secret_access_key: "secret".to_string(),
                        session_token: None,
                        region: "us-east-1".to_string(),
                    },
                    expires_at: Some(Instant::now() - Duration::from_secs(10)),
                    refresh_token: Some("refresh".to_string()),
                    provider_kind: ProviderKind::Bedrock,
                },
            );
        }

        let result = mgr.check_expiry(120);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ProviderKind::Bedrock);

        let event = rx.try_recv().unwrap();
        match event {
            AuthStatusEvent::Expired { provider } => {
                assert_eq!(provider, ProviderKind::Bedrock);
            }
            other => panic!("expected Expired, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_check_expiry_skips_no_refresh_token() {
        let (mgr, mut rx) = make_manager();

        // Near-expiry but NO refresh token -- should emit warning but NOT
        // include in the returned vec.
        mgr.cache_auth(
            ProviderKind::Azure,
            ProviderAuth::Azure {
                tenant_id: "tenant".to_string(),
                client_id: "client".to_string(),
                api_key: Some("azure-tok".to_string()),
            },
            Some(30),
            None, // no refresh token
        );

        let result = mgr.check_expiry(120);
        assert!(
            result.is_empty(),
            "should not include provider without refresh token"
        );

        // Warning event should still be emitted.
        let event = rx.try_recv().unwrap();
        match event {
            AuthStatusEvent::ExpiryWarning { provider, .. } => {
                assert_eq!(provider, ProviderKind::Azure);
            }
            other => panic!("expected ExpiryWarning, got: {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_has_refresh_token_true() {
        let (mgr, _rx) = make_manager();
        mgr.cache_auth(
            ProviderKind::Vertex,
            ProviderAuth::Gcp {
                access_token: "ya29.tok".to_string(),
            },
            Some(3600),
            Some("my-refresh-token".to_string()),
        );
        assert!(mgr.has_refresh_token(ProviderKind::Vertex));
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_has_refresh_token_false() {
        let (mgr, _rx) = make_manager();
        mgr.cache_auth(
            ProviderKind::Vertex,
            ProviderAuth::Gcp {
                access_token: "ya29.tok".to_string(),
            },
            Some(3600),
            None,
        );
        assert!(!mgr.has_refresh_token(ProviderKind::Vertex));
        // Also false for uncached provider.
        assert!(!mgr.has_refresh_token(ProviderKind::Bedrock));
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_get_refresh_token_returns_value() {
        let (mgr, _rx) = make_manager();
        mgr.cache_auth(
            ProviderKind::Bedrock,
            ProviderAuth::Aws {
                access_key_id: "AKIA".to_string(),
                secret_access_key: "secret".to_string(),
                session_token: None,
                region: "us-east-1".to_string(),
            },
            Some(43200),
            Some("bedrock-refresh-token".to_string()),
        );

        assert_eq!(
            mgr.get_refresh_token(ProviderKind::Bedrock),
            Some("bedrock-refresh-token".to_string())
        );
        // None for provider without refresh token or uncached.
        assert_eq!(mgr.get_refresh_token(ProviderKind::Vertex), None);
    }

    // rtmx:req REQ-LLM-037
    #[test]
    fn test_check_expiry_local_no_expiry() {
        let (mgr, mut rx) = make_manager();

        // Local provider has no expiry -- check_expiry should skip it entirely.
        mgr.cache_auth(ProviderKind::Local, ProviderAuth::NoAuth, None, None);

        let result = mgr.check_expiry(120);
        assert!(result.is_empty());

        // No events should have been emitted.
        assert!(rx.try_recv().is_err());
    }
}
