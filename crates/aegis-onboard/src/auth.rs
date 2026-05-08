//! Authentication utilities for OAuth/OIDC flows.
//!
//! Provides PKCE (Proof Key for Code Exchange) challenge/verifier generation
//! per RFC 7636 for secure authorization code flows.

use sha2::{Digest, Sha256};

/// Unreserved URI characters allowed in PKCE verifiers (RFC 7636, Section 4.1).
const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// Base64url alphabet (RFC 4648, Section 5).
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes as base64url without padding (RFC 4648, Section 5).
fn base64url_encode_no_pad(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64URL[((triple >> 18) & 0x3F) as usize] as char);
        out.push(BASE64URL[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64URL[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(BASE64URL[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

/// Generate a PKCE code verifier and challenge pair per RFC 7636.
///
/// Returns `(verifier, challenge)` where:
/// - `verifier`: 128-character random string using unreserved URI characters
/// - `challenge`: `BASE64URL(SHA256(verifier))` without padding
///
/// # Panics
///
/// Panics if the OS random number generator fails.
pub fn generate_pkce() -> (String, String) {
    // Generate random bytes for the verifier. We need 128 characters,
    // each selected from the UNRESERVED set via modular reduction.
    let mut random_bytes = [0u8; 128];
    getrandom::fill(&mut random_bytes).expect("OS RNG must be available");

    let verifier: String = random_bytes
        .iter()
        .map(|&b| UNRESERVED[(b as usize) % UNRESERVED.len()] as char)
        .collect();

    // challenge = BASE64URL(SHA256(verifier)) without padding
    let hash = Sha256::digest(verifier.as_bytes());
    let challenge = base64url_encode_no_pad(&hash);

    (verifier, challenge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    // rtmx:req REQ-ONBOARD-029
    #[test]
    fn test_pkce_challenge_verifier_pair() {
        let (verifier, challenge) = generate_pkce();

        // Verifier must be 43-128 characters (RFC 7636 Section 4.1).
        assert!(
            (43..=128).contains(&verifier.len()),
            "verifier length {} not in 43..=128",
            verifier.len()
        );

        // Verifier must only contain unreserved URI characters.
        for ch in verifier.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~'),
                "verifier contains invalid character: {ch:?}"
            );
        }

        // Challenge must be valid base64url (no padding).
        for ch in challenge.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'),
                "challenge contains invalid base64url character: {ch:?}"
            );
        }
        assert!(
            !challenge.contains('='),
            "challenge must not contain padding"
        );

        // Challenge must equal BASE64URL(SHA256(verifier)).
        let expected_hash = Sha256::digest(verifier.as_bytes());
        let expected_challenge = base64url_encode_no_pad(&expected_hash);
        assert_eq!(
            challenge, expected_challenge,
            "challenge must be BASE64URL(SHA256(verifier))"
        );

        // SHA-256 produces 32 bytes -> 43 base64url characters (no padding).
        assert_eq!(challenge.len(), 43, "SHA-256 base64url should be 43 chars");
    }

    // rtmx:req REQ-ONBOARD-029
    #[test]
    fn test_pkce_verifiers_are_unique() {
        let (v1, _) = generate_pkce();
        let (v2, _) = generate_pkce();
        assert_ne!(v1, v2, "successive verifiers must differ");
    }

    // rtmx:req REQ-ONBOARD-029
    #[test]
    fn test_pkce_base64url_no_standard_chars() {
        // Ensure challenge uses URL-safe alphabet (- and _ instead of + and /).
        for _ in 0..10 {
            let (_, challenge) = generate_pkce();
            assert!(!challenge.contains('+'), "must use - not +");
            assert!(!challenge.contains('/'), "must use _ not /");
        }
    }
}
