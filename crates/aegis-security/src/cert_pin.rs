//! Certificate pinning for government LLM endpoints.
//!
//! Validates that TLS certificate SHA-256 fingerprints match
//! pre-configured pins, preventing MITM attacks even when a
//! rogue CA is trusted by the OS (REQ-SECURITY-007).

use std::collections::HashMap;
use thiserror::Error;
use tracing::warn;

/// Error returned when certificate pin verification fails.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CertPinError {
    #[error("certificate pin mismatch for {host}: expected one of [{expected}], got {actual}")]
    PinMismatch {
        host: String,
        expected: String,
        actual: String,
    },
}

/// Certificate pinning configuration.
///
/// Maps hostnames (or wildcard patterns like `*.googleapis.com`) to
/// one or more expected SHA-256 fingerprints. Multiple pins per host
/// support certificate rotation without downtime.
#[derive(Debug, Clone)]
pub struct CertPinConfig {
    /// Map of hostname -> expected SHA-256 fingerprint(s).
    pub pins: HashMap<String, Vec<String>>,
    /// Whether to enforce pins (false = log-only mode for testing).
    pub enforce: bool,
}

impl Default for CertPinConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl CertPinConfig {
    /// Create a new empty config with enforcement enabled.
    pub fn new() -> Self {
        Self {
            pins: HashMap::new(),
            enforce: true,
        }
    }

    /// Add a SHA-256 fingerprint pin for a host.
    pub fn add_pin(&mut self, host: &str, sha256_fingerprint: &str) {
        self.pins
            .entry(host.to_string())
            .or_default()
            .push(sha256_fingerprint.to_string());
    }

    /// Check whether a host has any pinned fingerprints.
    pub fn is_pinned(&self, host: &str) -> bool {
        self.find_pins(host).is_some()
    }

    /// Verify a certificate fingerprint against pinned values.
    ///
    /// Returns `Ok(())` when:
    /// - The host is not pinned (no pins configured for it), or
    /// - The fingerprint matches any of the pinned values.
    ///
    /// Returns `Err(CertPinError::PinMismatch)` when the host is
    /// pinned, enforcement is on, and no pin matches.
    ///
    /// When `enforce` is `false`, a mismatch logs a warning but
    /// returns `Ok(())`.
    pub fn verify(&self, host: &str, cert_fingerprint: &str) -> Result<(), CertPinError> {
        let pins = match self.find_pins(host) {
            Some(pins) => pins,
            None => return Ok(()),
        };

        if pins.iter().any(|p| p == cert_fingerprint) {
            return Ok(());
        }

        let expected = pins.join(", ");
        if self.enforce {
            Err(CertPinError::PinMismatch {
                host: host.to_string(),
                expected,
                actual: cert_fingerprint.to_string(),
            })
        } else {
            warn!(
                host = host,
                expected = %expected,
                actual = cert_fingerprint,
                "certificate pin mismatch (enforce=false, allowing)"
            );
            Ok(())
        }
    }

    /// Find pins matching a host, checking exact match first, then wildcards.
    fn find_pins(&self, host: &str) -> Option<&Vec<String>> {
        // Exact match first.
        if let Some(pins) = self.pins.get(host) {
            return Some(pins);
        }

        // Check wildcard patterns: `*.example.com` matches `foo.example.com`.
        for (pattern, pins) in &self.pins {
            if let Some(suffix) = pattern.strip_prefix("*.")
                && host.ends_with(suffix)
                && host.len() > suffix.len()
                && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
            {
                return Some(pins);
            }
        }

        None
    }
}

/// Return default certificate pins for known government endpoints.
///
/// Fingerprints are placeholders -- replace with actual SHA-256
/// certificate fingerprints before production deployment.
pub fn default_gov_pins() -> CertPinConfig {
    let mut config = CertPinConfig::new();

    // TODO: Replace with actual Google public CA SHA-256 fingerprint.
    config.add_pin(
        "*.googleapis.com",
        "TODO:replace-with-actual-google-public-ca-sha256-fingerprint",
    );

    // TODO: Replace with actual AWS GovCloud endpoint fingerprint.
    // config.add_pin("*.amazonaws.com", "TODO:...");

    // TODO: Replace with actual Azure Government endpoint fingerprint.
    // config.add_pin("*.azure.us", "TODO:...");

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    const FP_A: &str = "sha256/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    const FP_B: &str = "sha256/BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=";
    const FP_BAD: &str = "sha256/XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn add_pin_stores_fingerprint() {
        let mut config = CertPinConfig::new();
        config.add_pin("example.com", FP_A);

        assert_eq!(config.pins["example.com"], vec![FP_A]);
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn is_pinned_returns_true_for_pinned_host() {
        let mut config = CertPinConfig::new();
        config.add_pin("example.com", FP_A);

        assert!(config.is_pinned("example.com"));
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn is_pinned_returns_false_for_unpinned_host() {
        let config = CertPinConfig::new();

        assert!(!config.is_pinned("example.com"));
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn verify_succeeds_with_matching_fingerprint() {
        let mut config = CertPinConfig::new();
        config.add_pin("example.com", FP_A);

        assert!(config.verify("example.com", FP_A).is_ok());
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn verify_fails_with_mismatched_fingerprint() {
        let mut config = CertPinConfig::new();
        config.add_pin("example.com", FP_A);

        let result = config.verify("example.com", FP_BAD);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let CertPinError::PinMismatch {
            host,
            expected,
            actual,
        } = &err;
        assert_eq!(host, "example.com");
        assert!(expected.contains(FP_A));
        assert_eq!(actual, FP_BAD);
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn multiple_pins_per_host_any_match_succeeds() {
        let mut config = CertPinConfig::new();
        config.add_pin("example.com", FP_A);
        config.add_pin("example.com", FP_B);

        // Either pin should be accepted.
        assert!(config.verify("example.com", FP_A).is_ok());
        assert!(config.verify("example.com", FP_B).is_ok());
        assert!(config.verify("example.com", FP_BAD).is_err());
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn enforce_false_logs_but_does_not_fail() {
        let mut config = CertPinConfig::new();
        config.enforce = false;
        config.add_pin("example.com", FP_A);

        // Mismatch should succeed when enforce is false.
        assert!(config.verify("example.com", FP_BAD).is_ok());
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn wildcard_pin_matches_subdomain() {
        let mut config = CertPinConfig::new();
        config.add_pin("*.googleapis.com", FP_A);

        assert!(config.is_pinned("vertex.googleapis.com"));
        assert!(config.verify("vertex.googleapis.com", FP_A).is_ok());
        assert!(config.verify("vertex.googleapis.com", FP_BAD).is_err());
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn wildcard_pin_does_not_match_bare_domain() {
        let mut config = CertPinConfig::new();
        config.add_pin("*.googleapis.com", FP_A);

        // The bare domain itself should NOT match a wildcard pin.
        assert!(!config.is_pinned("googleapis.com"));
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn unpinned_host_verify_always_succeeds() {
        let config = CertPinConfig::new();

        // No pins configured, so any fingerprint is accepted.
        assert!(config.verify("example.com", FP_BAD).is_ok());
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn default_gov_pins_includes_googleapis() {
        let config = default_gov_pins();

        assert!(config.is_pinned("vertex.googleapis.com"));
        assert!(config.is_pinned("us-central1-aiplatform.googleapis.com"));
    }

    // rtmx:req REQ-SECURITY-007
    #[test]
    fn default_impl_matches_new() {
        let default_config = CertPinConfig::default();
        let new_config = CertPinConfig::new();

        assert_eq!(default_config.pins.len(), new_config.pins.len());
        assert_eq!(default_config.enforce, new_config.enforce);
    }
}
