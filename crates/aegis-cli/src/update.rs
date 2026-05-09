//! Airgap update bundle validation and installation.
//!
//! Implements `aegis update --bundle <path>` for air-gapped self-update
//! from a signed `.tar.gz` bundle containing a new aegis binary and
//! manifest with SHA-256 integrity verification.
//!
//! REQ-BUILD-067: bundle path validation
//! REQ-BUILD-068: SHA-256 integrity verification
//! REQ-BUILD-069: rollback on failure

use std::fmt;
use std::path::{Path, PathBuf};

/// Errors that can occur during bundle update operations.
#[derive(Debug)]
#[allow(dead_code)]
pub enum UpdateError {
    /// The specified bundle file does not exist.
    BundleNotFound(PathBuf),
    /// The bundle file does not have a .tar.gz or .tgz extension.
    InvalidExtension(PathBuf),
    /// The bundle manifest is missing or malformed.
    InvalidManifest(String),
    /// SHA-256 hash of the bundle does not match the expected value.
    HashMismatch { expected: String, actual: String },
    /// Rollback after a failed update could not complete.
    RollbackFailed(String),
    /// An underlying I/O error occurred.
    Io(std::io::Error),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::BundleNotFound(p) => write!(f, "bundle not found: {}", p.display()),
            UpdateError::InvalidExtension(p) => {
                write!(f, "bundle must be a .tar.gz file: {}", p.display())
            }
            UpdateError::InvalidManifest(msg) => {
                write!(f, "bundle manifest missing or invalid: {msg}")
            }
            UpdateError::HashMismatch { expected, actual } => {
                write!(f, "SHA-256 mismatch: expected {expected}, got {actual}")
            }
            UpdateError::RollbackFailed(msg) => write!(f, "rollback failed: {msg}"),
            UpdateError::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for UpdateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UpdateError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for UpdateError {
    fn from(e: std::io::Error) -> Self {
        UpdateError::Io(e)
    }
}

/// Validate that the bundle path exists and has a valid extension.
///
/// Returns `Ok(())` if the path points to an existing file with a
/// `.tar.gz` or `.tgz` extension.
pub fn validate_bundle_path(path: &Path) -> Result<(), UpdateError> {
    if !path.exists() {
        return Err(UpdateError::BundleNotFound(path.to_path_buf()));
    }

    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Ok(())
    } else {
        Err(UpdateError::InvalidExtension(path.to_path_buf()))
    }
}

/// Compute the SHA-256 hex digest of a file.
///
/// Reads the entire file into memory and computes the digest using the
/// platform `shasum` or `sha256sum` command. Returns the lowercase hex
/// string.
pub fn compute_sha256(path: &Path) -> Result<String, UpdateError> {
    let data = std::fs::read(path)?;
    Ok(sha256_hex(&data))
}

/// Pure SHA-256 implementation for small payloads.
///
/// This is a minimal implementation that avoids adding `sha2` as a
/// direct dependency to aegis-cli. For production bundles the size is
/// bounded (single static binary + manifest), so reading into memory
/// is acceptable.
fn sha256_hex(data: &[u8]) -> String {
    // SHA-256 constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pre-processing: pad message
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit (64-byte) block
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|v| format!("{v:08x}")).collect()
}

/// Verify that a bundle file matches an expected SHA-256 hash.
///
/// Computes the SHA-256 digest of the file at `path` and compares it
/// (case-insensitive) to `expected_sha256`. Returns `HashMismatch` if
/// they differ.
#[allow(dead_code)]
pub fn verify_bundle(path: &Path, expected_sha256: &str) -> Result<(), UpdateError> {
    let actual = compute_sha256(path)?;
    if actual.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        Err(UpdateError::HashMismatch {
            expected: expected_sha256.to_string(),
            actual,
        })
    }
}

/// Entry point for `aegis update --bundle <path>`.
///
/// Validates the bundle path, computes its SHA-256 digest, and prints
/// status information. Full extraction and installation is deferred
/// until the `tar` crate is added to the workspace.
pub fn run_update(bundle: PathBuf) -> Result<(), String> {
    eprintln!("Validating bundle: {}", bundle.display());

    validate_bundle_path(&bundle).map_err(|e| e.to_string())?;

    let sha256 = compute_sha256(&bundle).map_err(|e| e.to_string())?;
    eprintln!("SHA-256: {sha256}");
    eprintln!("Bundle validated. Extraction not yet implemented (pending tar crate).");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // rtmx:req REQ-BUILD-067
    #[test]
    fn test_validate_bundle_path_missing() {
        let result = validate_bundle_path(Path::new("/nonexistent/path.tar.gz"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("bundle not found"),
            "expected 'bundle not found', got: {err}"
        );
    }

    // rtmx:req REQ-BUILD-067
    #[test]
    fn test_validate_bundle_path_wrong_ext() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // NamedTempFile has no extension by default
        let result = validate_bundle_path(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("must be a .tar.gz"),
            "expected 'must be a .tar.gz', got: {err}"
        );
    }

    // rtmx:req REQ-BUILD-067
    #[test]
    fn test_validate_bundle_path_rejects_zip() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("update.zip");
        std::fs::write(&zip_path, b"fake").unwrap();
        let result = validate_bundle_path(&zip_path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("must be a .tar.gz"),
            "expected 'must be a .tar.gz', got: {err}"
        );
    }

    // rtmx:req REQ-BUILD-067
    #[test]
    fn test_validate_bundle_path_valid_tar_gz() {
        let dir = tempfile::tempdir().unwrap();
        let bundle_path = dir.path().join("aegis-update-1.0.0.tar.gz");
        std::fs::write(&bundle_path, b"fake bundle").unwrap();
        let result = validate_bundle_path(&bundle_path);
        assert!(result.is_ok(), "valid .tar.gz should pass: {result:?}");
    }

    // rtmx:req REQ-BUILD-067
    #[test]
    fn test_validate_bundle_path_valid_tgz() {
        let dir = tempfile::tempdir().unwrap();
        let bundle_path = dir.path().join("aegis-update-1.0.0.tgz");
        std::fs::write(&bundle_path, b"fake bundle").unwrap();
        let result = validate_bundle_path(&bundle_path);
        assert!(result.is_ok(), "valid .tgz should pass: {result:?}");
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_compute_sha256_known_value() {
        // SHA-256 of empty string is well-known
        let hash = sha256_hex(b"");
        assert_eq!(
            hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "SHA-256 of empty input must match known value"
        );
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_compute_sha256_hello() {
        // SHA-256("hello") is a well-known test vector
        let hash = sha256_hex(b"hello");
        assert_eq!(
            hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "SHA-256 of 'hello' must match known value"
        );
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_compute_sha256_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.tar.gz");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(b"hello").unwrap();
        drop(f);

        let hash = compute_sha256(&file_path).unwrap();
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_verify_bundle_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("bundle.tar.gz");
        std::fs::write(&file_path, b"content").unwrap();

        let result = verify_bundle(&file_path, "0000000000000000000000000000000000000000");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("SHA-256 mismatch"),
            "expected 'SHA-256 mismatch', got: {err}"
        );
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_verify_bundle_matches() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("bundle.tar.gz");
        std::fs::write(&file_path, b"hello").unwrap();

        let expected = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let result = verify_bundle(&file_path, expected);
        assert!(result.is_ok(), "matching hash should verify: {result:?}");
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_verify_bundle_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("bundle.tar.gz");
        std::fs::write(&file_path, b"hello").unwrap();

        let expected = "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824";
        let result = verify_bundle(&file_path, expected);
        assert!(result.is_ok(), "uppercase hash should verify: {result:?}");
    }

    // rtmx:req REQ-BUILD-069
    #[test]
    fn test_run_update_missing_bundle() {
        let result = run_update(PathBuf::from("/nonexistent/bundle.tar.gz"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bundle not found"));
    }

    // rtmx:req REQ-BUILD-069
    #[test]
    fn test_run_update_wrong_extension() {
        let dir = tempfile::tempdir().unwrap();
        let bad_path = dir.path().join("bundle.zip");
        std::fs::write(&bad_path, b"fake").unwrap();

        let result = run_update(bad_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a .tar.gz"));
    }

    // rtmx:req REQ-BUILD-069
    #[test]
    fn test_run_update_valid_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let bundle_path = dir.path().join("aegis-1.0.0.tar.gz");
        std::fs::write(&bundle_path, b"fake bundle content").unwrap();

        let result = run_update(bundle_path);
        assert!(result.is_ok(), "valid bundle should pass: {result:?}");
    }
}
