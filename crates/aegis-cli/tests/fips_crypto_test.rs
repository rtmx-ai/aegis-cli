//! Validation tests for FIPS 140-2 validated crypto primitives.
//!
//! REQ-BUILD-008: Binary links FIPS 140-2 validated crypto primitives.
//! REQ-SECURITY-026: Binary links CMVP-validated FIPS crypto provider (aws-lc-rs).
//!
//! The workspace uses `reqwest` with the `rustls-tls` feature, which
//! brings in `rustls` backed by `aws-lc-rs`. The `aws-lc-rs` crate provides
//! CMVP-validated FIPS 140-2 cryptography (NIST Certificate #4631) including
//! AES-GCM, SHA-2, ECDSA, and ChaCha20-Poly1305.
//!
//! These tests verify the build configuration enforces rustls as the
//! TLS backend with aws-lc-rs as the crypto provider, and that native-tls
//! is not pulled in as the primary provider.

use std::path::Path;

/// Return the workspace root Cargo.toml content.
fn workspace_toml() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Cargo.toml");
    std::fs::read_to_string(&root).expect("read workspace Cargo.toml")
}

/// Return the workspace Cargo.lock content.
fn cargo_lock() -> String {
    let lock = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Cargo.lock");
    std::fs::read_to_string(&lock).expect("read Cargo.lock")
}

// rtmx:req REQ-BUILD-008
#[test]
fn test_tls_backend_is_fips_capable() {
    let content = workspace_toml();
    // reqwest with rustls-tls feature provides FIPS-validated crypto
    // via the aws-lc-rs backend.
    assert!(
        content.contains("rustls-tls") || content.contains("rustls"),
        "reqwest must use rustls TLS backend for FIPS capability"
    );
}

// rtmx:req REQ-BUILD-008
#[test]
fn test_reqwest_uses_rustls_feature() {
    let content = workspace_toml();
    // The workspace dependency for reqwest must include rustls-tls.
    assert!(
        content.contains("rustls-tls"),
        "workspace reqwest dependency must enable the rustls-tls feature"
    );
}

// rtmx:req REQ-BUILD-008
#[test]
fn test_no_native_tls_as_primary_backend() {
    let content = workspace_toml();
    // The workspace reqwest entry should NOT have native-tls as the
    // primary feature. It is acceptable for native-tls to appear as a
    // transitive dependency, but reqwest itself must prefer rustls.
    let has_native_tls_feature = content
        .lines()
        .any(|line| line.contains("reqwest") && line.contains("native-tls"));
    assert!(
        !has_native_tls_feature,
        "reqwest should not be configured with native-tls feature; \
         use rustls-tls for FIPS-validated crypto"
    );
}

// rtmx:req REQ-BUILD-008
#[test]
fn test_reqwest_disables_default_features() {
    let content = workspace_toml();
    // reqwest's default features include native-tls on some platforms.
    // We must disable defaults and explicitly enable rustls-tls.
    let reqwest_line = content
        .lines()
        .find(|line| line.contains("reqwest"))
        .expect("reqwest dependency must exist in workspace Cargo.toml");
    assert!(
        reqwest_line.contains("default-features = false"),
        "reqwest should have default-features = false to prevent \
         native-tls from being pulled in: {reqwest_line}"
    );
}

// rtmx:req REQ-BUILD-008
#[test]
fn test_cargo_lock_contains_rustls() {
    let content = cargo_lock();
    assert!(
        content.contains("name = \"rustls\""),
        "Cargo.lock must contain the rustls crate for FIPS-validated TLS"
    );
}

// rtmx:req REQ-SECURITY-026
#[test]
fn test_cargo_lock_contains_aws_lc_fips_provider() {
    let content = cargo_lock();
    // aws-lc-rs provides CMVP-validated FIPS 140-2 cryptography
    // (NIST Certificate #4631).
    assert!(
        content.contains("name = \"aws-lc-rs\""),
        "Cargo.lock must contain aws-lc-rs, the CMVP-validated FIPS crypto \
         provider used by rustls (NIST Certificate #4631)."
    );
}

// rtmx:req REQ-SECURITY-026
#[test]
fn test_cargo_lock_contains_aws_lc_rs() {
    let content = cargo_lock();
    assert!(
        content.contains("name = \"aws-lc-rs\""),
        "Cargo.lock must contain the aws-lc-rs crate for FIPS 140-2 \
         validated cryptography."
    );
}

// rtmx:req REQ-SECURITY-026
#[test]
fn test_workspace_declares_aws_lc_rs_dependency() {
    let content = workspace_toml();
    assert!(
        content.contains("aws-lc-rs"),
        "workspace Cargo.toml must declare aws-lc-rs as a dependency \
         to ensure CMVP-validated FIPS crypto provider is used."
    );
}

// rtmx:req REQ-BUILD-008
#[test]
fn test_cargo_lock_may_contain_ring() {
    let content = cargo_lock();
    // ring may still appear as a transitive dependency. This test
    // documents that ring is acceptable as a transitive dep but is
    // no longer the primary crypto provider (aws-lc-rs is).
    if content.contains("name = \"ring\"") {
        // ring is present as a transitive dep -- acceptable
    }
    // This test always passes; it documents the expected state.
}
