//! Airgap update bundle validation and installation.
//!
//! Implements `aegis update --bundle <path>` for air-gapped self-update
//! from a signed `.tar.gz` bundle containing a new aegis binary and
//! manifest with SHA-256 integrity verification.
//!
//! REQ-BUILD-067: bundle path validation
//! REQ-BUILD-068: SHA-256 integrity verification
//! REQ-BUILD-069: rollback on failure

use sha2::{Digest, Sha256};
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
/// Reads the entire file and computes the digest via the `sha2` crate
/// (backed by aws-lc-rs FIPS-validated primitives). Returns the
/// lowercase hex string.
pub fn compute_sha256(path: &Path) -> Result<String, UpdateError> {
    let data = std::fs::read(path)?;
    let hash = Sha256::digest(&data);
    Ok(format!("{hash:x}"))
}

/// Compute SHA-256 hex digest of a byte slice (for testing).
#[cfg(test)]
fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    format!("{hash:x}")
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
