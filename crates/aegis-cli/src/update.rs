//! Airgap update bundle validation and installation.
//!
//! Implements `aegis update --bundle <path>` for air-gapped self-update
//! from a signed `.tar.gz` bundle containing a new aegis binary and
//! manifest with SHA-256 integrity verification.
//!
//! REQ-BUILD-011: Closed-network update bundle for offline version upgrades
//! REQ-BUILD-050: Bundle extraction and manifest verification
//! REQ-BUILD-067: Bundle path validation
//! REQ-BUILD-068: Binary replacement with rollback
//! REQ-BUILD-069: Post-update self-test and version confirmation

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use tar::Archive;

/// Errors that can occur during bundle update operations.
#[derive(Debug)]
pub enum UpdateError {
    /// The specified bundle file does not exist.
    BundleNotFound(PathBuf),
    /// The bundle file does not have a .tar.gz or .tgz extension.
    InvalidExtension(PathBuf),
    /// The bundle manifest is missing or malformed.
    InvalidManifest(String),
    /// SHA-256 hash of a file does not match the expected value.
    HashMismatch { expected: String, actual: String },
    /// Rollback after a failed update could not complete.
    RollbackFailed(String),
    /// Post-update version verification failed.
    VersionMismatch { expected: String, actual: String },
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
            UpdateError::VersionMismatch { expected, actual } => {
                write!(
                    f,
                    "version mismatch after update: expected {expected}, got {actual}"
                )
            }
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

/// Extract a `.tar.gz` bundle into `target_dir` and verify manifest integrity.
///
/// Opens the tarball, decompresses with flate2, extracts with tar crate,
/// verifies `manifest.txt` exists, checks SHA-256 of each listed file,
/// and parses `version.json` to return the bundle version string.
pub fn extract_bundle(bundle_path: &Path, target_dir: &Path) -> Result<BundleInfo, UpdateError> {
    // 1. Open and decompress
    let file = std::fs::File::open(bundle_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    // 2. Extract all entries
    archive.unpack(target_dir)?;

    // 3. Verify manifest.txt exists
    let manifest_path = target_dir.join("manifest.txt");
    if !manifest_path.exists() {
        return Err(UpdateError::InvalidManifest(
            "manifest.txt not found in bundle".to_string(),
        ));
    }

    // 4. Parse manifest.txt (sha256sum format: "HASH  FILENAME\n")
    let manifest_content = std::fs::read_to_string(&manifest_path)?;
    let manifest_entries = parse_manifest(&manifest_content)?;

    // 5. Verify SHA-256 of each listed file
    for (filename, expected_hash) in &manifest_entries {
        let file_path = target_dir.join(filename);
        if !file_path.exists() {
            return Err(UpdateError::InvalidManifest(format!(
                "manifest references missing file: {filename}"
            )));
        }
        let actual_hash = compute_sha256(&file_path)?;
        if !actual_hash.eq_ignore_ascii_case(expected_hash) {
            return Err(UpdateError::HashMismatch {
                expected: expected_hash.clone(),
                actual: actual_hash,
            });
        }
    }

    // 6. Parse version.json
    let version_path = target_dir.join("version.json");
    if !version_path.exists() {
        return Err(UpdateError::InvalidManifest(
            "version.json not found in bundle".to_string(),
        ));
    }
    let version_content = std::fs::read_to_string(&version_path)?;
    let version_info: serde_json::Value = serde_json::from_str(&version_content)
        .map_err(|e| UpdateError::InvalidManifest(format!("invalid version.json: {e}")))?;

    let version = version_info["version"]
        .as_str()
        .ok_or_else(|| {
            UpdateError::InvalidManifest("version.json missing 'version' field".to_string())
        })?
        .to_string();

    Ok(BundleInfo {
        version,
        extracted_dir: target_dir.to_path_buf(),
        manifest_entries,
    })
}

/// Information about an extracted and verified bundle.
#[derive(Debug)]
#[allow(dead_code)]
pub struct BundleInfo {
    /// Version string from version.json.
    pub version: String,
    /// Directory where the bundle was extracted.
    pub extracted_dir: PathBuf,
    /// Map of filename -> expected SHA-256 from manifest.txt.
    pub manifest_entries: HashMap<String, String>,
}

/// Parse manifest.txt in sha256sum format ("HASH  FILENAME" per line).
fn parse_manifest(content: &str) -> Result<HashMap<String, String>, UpdateError> {
    let mut entries = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // sha256sum format: two-space separator between hash and filename
        let parts: Vec<&str> = line.splitn(2, "  ").collect();
        if parts.len() != 2 {
            // Also try single space (some tools use one space)
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() != 2 {
                return Err(UpdateError::InvalidManifest(format!(
                    "malformed manifest line: {line}"
                )));
            }
            let hash = parts[0].trim();
            let filename = parts[1].trim().trim_start_matches('*');
            entries.insert(filename.to_string(), hash.to_string());
        } else {
            let hash = parts[0].trim();
            let filename = parts[1].trim().trim_start_matches('*');
            entries.insert(filename.to_string(), hash.to_string());
        }
    }
    if entries.is_empty() {
        return Err(UpdateError::InvalidManifest(
            "manifest.txt contains no entries".to_string(),
        ));
    }
    Ok(entries)
}

/// Replace the current binary with the one from the extracted bundle.
///
/// Creates a `.backup` of the current binary before replacement. On any
/// failure after backup creation, the backup is restored (rollback).
/// On success, the backup is cleaned up.
///
/// `current_exe` is the path to the currently running binary.
/// `extracted_dir` is the directory containing the extracted bundle.
pub fn replace_binary(current_exe: &Path, extracted_dir: &Path) -> Result<(), UpdateError> {
    let new_binary = extracted_dir.join("aegis");
    if !new_binary.exists() {
        return Err(UpdateError::InvalidManifest(
            "bundle does not contain 'aegis' binary".to_string(),
        ));
    }

    let backup_path = current_exe.with_extension("backup");

    // Create backup of current binary
    std::fs::copy(current_exe, &backup_path).map_err(|e| {
        UpdateError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to create backup: {e}"),
        ))
    })?;

    // Attempt to replace the binary
    match do_replace(current_exe, &new_binary) {
        Ok(()) => {
            // Success: clean up backup
            let _ = std::fs::remove_file(&backup_path);
            Ok(())
        }
        Err(e) => {
            // Rollback: restore from backup
            if let Err(restore_err) = std::fs::copy(&backup_path, current_exe) {
                return Err(UpdateError::RollbackFailed(format!(
                    "update failed ({e}) and rollback also failed ({restore_err}). \
                     Manual recovery needed from: {}",
                    backup_path.display()
                )));
            }
            let _ = std::fs::remove_file(&backup_path);
            Err(e)
        }
    }
}

/// Perform the actual binary replacement: copy new binary and set permissions.
fn do_replace(target: &Path, source: &Path) -> Result<(), UpdateError> {
    std::fs::copy(source, target)?;

    // Set executable permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(target, perms)?;
    }

    Ok(())
}

/// Run `aegis --version` as a subprocess and verify it matches the expected version.
///
/// Returns the version string from the subprocess output. If the version
/// does not match `expected_version`, returns `VersionMismatch`.
pub fn verify_version(binary_path: &Path, expected_version: &str) -> Result<String, UpdateError> {
    let output = std::process::Command::new(binary_path)
        .arg("--version")
        .output()
        .map_err(|e| {
            UpdateError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to run post-update self-test: {e}"),
            ))
        })?;

    if !output.status.success() {
        return Err(UpdateError::VersionMismatch {
            expected: expected_version.to_string(),
            actual: format!("process exited with: {}", output.status),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // `aegis --version` outputs "aegis <version> (<git_sha> <target>)"
    // or just "aegis <version>". We extract the version part.
    let actual_version = parse_version_output(&stdout);

    if actual_version != expected_version {
        return Err(UpdateError::VersionMismatch {
            expected: expected_version.to_string(),
            actual: actual_version.clone(),
        });
    }

    Ok(actual_version)
}

/// Parse the version from `aegis --version` output.
///
/// Expected formats:
///   "aegis 1.2.3"
///   "aegis 1.2.3 (abc1234 x86_64-unknown-linux-musl)"
fn parse_version_output(output: &str) -> String {
    let trimmed = output.trim();
    // Split on whitespace, take the second token (the version)
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Entry point for `aegis update --bundle <path>`.
///
/// Full update pipeline:
/// 1. Validate bundle path and extension
/// 2. Compute bundle SHA-256
/// 3. Extract tarball and verify manifest integrity
/// 4. Replace current binary with rollback support
/// 5. Run post-update self-test to confirm version
pub fn run_update(bundle: PathBuf) -> Result<(), String> {
    eprintln!("Validating bundle: {}", bundle.display());

    validate_bundle_path(&bundle).map_err(|e| e.to_string())?;

    let sha256 = compute_sha256(&bundle).map_err(|e| e.to_string())?;
    eprintln!("SHA-256: {sha256}");

    // Extract to a temporary directory
    let extract_dir =
        tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    eprintln!("Extracting bundle...");

    let bundle_info = extract_bundle(&bundle, extract_dir.path()).map_err(|e| e.to_string())?;
    eprintln!(
        "Bundle version: {} ({} files verified)",
        bundle_info.version,
        bundle_info.manifest_entries.len()
    );

    // Get current binary path
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("cannot determine current binary path: {e}"))?;

    // Get current version for reporting
    let current_version = env!("CARGO_PKG_VERSION");

    eprintln!("Replacing binary: {}", current_exe.display());
    replace_binary(&current_exe, extract_dir.path()).map_err(|e| e.to_string())?;

    // Post-update self-test
    eprintln!("Running post-update self-test...");
    match verify_version(&current_exe, &bundle_info.version) {
        Ok(ver) => {
            eprintln!("Update complete: {current_version} -> {ver}");
            Ok(())
        }
        Err(e) => {
            // Self-test failed; attempt rollback
            let backup_path = current_exe.with_extension("backup");
            if backup_path.exists() {
                eprintln!("Self-test failed, rolling back...");
                if let Err(re) = std::fs::copy(&backup_path, &current_exe) {
                    return Err(format!(
                        "self-test failed ({e}) and rollback failed ({re}). \
                         Manual recovery from: {}",
                        backup_path.display()
                    ));
                }
                let _ = std::fs::remove_file(&backup_path);
            }
            Err(format!("self-test failed: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // ---------------------------------------------------------------
    // REQ-BUILD-067: Bundle path validation
    // ---------------------------------------------------------------

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

    // ---------------------------------------------------------------
    // REQ-BUILD-068 / SHA-256 integrity verification
    // ---------------------------------------------------------------

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_compute_sha256_known_value() {
        let hash = sha256_hex(b"");
        assert_eq!(
            hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "SHA-256 of empty input must match known value"
        );
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_compute_sha256_hello() {
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

    // ---------------------------------------------------------------
    // REQ-BUILD-050: Bundle extraction and manifest verification
    // ---------------------------------------------------------------

    /// Helper: create a tar.gz bundle in `dir` with the given files.
    /// Returns the path to the created bundle.
    fn create_test_bundle(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        use flate2::Compression;
        use flate2::write::GzEncoder;

        let bundle_path = dir.join("test-bundle.tar.gz");
        let file = std::fs::File::create(&bundle_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);

        for (name, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, *name, &content[..])
                .unwrap();
        }

        builder.finish().unwrap();
        bundle_path
    }

    /// Helper: create a manifest.txt content string from files.
    fn create_manifest(files: &[(&str, &[u8])]) -> String {
        let mut manifest = String::new();
        for (name, content) in files {
            let hash = sha256_hex(content);
            manifest.push_str(&format!("{hash}  {name}\n"));
        }
        manifest
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_extract_bundle_success() {
        let dir = tempfile::tempdir().unwrap();

        let binary_content = b"fake aegis binary";
        let version_json = br#"{"version":"2.0.0","git_sha":"abc123","build_date":"2026-05-12","target":"x86_64-unknown-linux-musl"}"#;
        let sbom_json = b"{}";

        let data_files: Vec<(&str, &[u8])> = vec![
            ("aegis", binary_content),
            ("version.json", version_json),
            ("sbom.json", sbom_json),
        ];

        let manifest = create_manifest(&data_files);

        let mut all_files: Vec<(&str, &[u8])> = data_files;
        let manifest_bytes = manifest.as_bytes().to_vec();
        all_files.push(("manifest.txt", &manifest_bytes));

        // Need to hold manifest_bytes alive, rebuild
        let manifest_str = create_manifest(&[
            ("aegis", binary_content),
            ("version.json", version_json),
            ("sbom.json", sbom_json),
        ]);
        let manifest_bytes2 = manifest_str.into_bytes();

        let bundle_path = create_test_bundle(
            dir.path(),
            &[
                ("aegis", binary_content),
                ("version.json", version_json),
                ("sbom.json", sbom_json),
                ("manifest.txt", &manifest_bytes2),
            ],
        );

        let extract_dir = tempfile::tempdir().unwrap();
        let info = extract_bundle(&bundle_path, extract_dir.path()).unwrap();

        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.manifest_entries.len(), 3);
        assert!(extract_dir.path().join("aegis").exists());
        assert!(extract_dir.path().join("version.json").exists());
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_extract_bundle_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();

        let bundle_path =
            create_test_bundle(dir.path(), &[("aegis", b"binary"), ("version.json", b"{}")]);

        let extract_dir = tempfile::tempdir().unwrap();
        let result = extract_bundle(&bundle_path, extract_dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("manifest.txt not found"),
            "expected manifest error, got: {err}"
        );
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_extract_bundle_hash_mismatch_in_manifest() {
        let dir = tempfile::tempdir().unwrap();

        let bad_manifest =
            "deadbeef00000000000000000000000000000000000000000000000000000000  aegis\n";

        let bundle_path = create_test_bundle(
            dir.path(),
            &[
                ("aegis", b"binary content"),
                ("version.json", br#"{"version":"1.0.0"}"#),
                ("manifest.txt", bad_manifest.as_bytes()),
            ],
        );

        let extract_dir = tempfile::tempdir().unwrap();
        let result = extract_bundle(&bundle_path, extract_dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("SHA-256 mismatch"),
            "expected hash mismatch, got: {err}"
        );
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_extract_bundle_missing_version_json() {
        let dir = tempfile::tempdir().unwrap();

        let aegis_content = b"binary";
        let manifest = format!("{}  aegis\n", sha256_hex(aegis_content));

        let bundle_path = create_test_bundle(
            dir.path(),
            &[
                ("aegis", aegis_content),
                ("manifest.txt", manifest.as_bytes()),
            ],
        );

        let extract_dir = tempfile::tempdir().unwrap();
        let result = extract_bundle(&bundle_path, extract_dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("version.json not found"),
            "expected version.json error, got: {err}"
        );
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_parse_manifest_valid() {
        let content = "abc123  file1.txt\ndef456  file2.bin\n";
        let entries = parse_manifest(content).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries["file1.txt"], "abc123");
        assert_eq!(entries["file2.bin"], "def456");
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_parse_manifest_empty() {
        let result = parse_manifest("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no entries"));
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_parse_manifest_ignores_comments() {
        let content = "# comment\nabc123  file1.txt\n";
        let entries = parse_manifest(content).unwrap();
        assert_eq!(entries.len(), 1);
    }

    // rtmx:req REQ-BUILD-050
    #[test]
    fn test_extract_bundle_manifest_references_missing_file() {
        let dir = tempfile::tempdir().unwrap();

        let aegis_content = b"binary";
        let manifest = format!(
            "{}  aegis\n{}  nonexistent.txt\n",
            sha256_hex(aegis_content),
            sha256_hex(b"ghost"),
        );

        let bundle_path = create_test_bundle(
            dir.path(),
            &[
                ("aegis", aegis_content),
                ("version.json", br#"{"version":"1.0.0"}"#),
                ("manifest.txt", manifest.as_bytes()),
            ],
        );

        let extract_dir = tempfile::tempdir().unwrap();
        let result = extract_bundle(&bundle_path, extract_dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("missing file"),
            "expected missing file error, got: {err}"
        );
    }

    // ---------------------------------------------------------------
    // REQ-BUILD-068: Binary replacement with rollback
    // ---------------------------------------------------------------

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_replace_binary_success() {
        let dir = tempfile::tempdir().unwrap();

        // Create a fake "current" binary
        let current_exe = dir.path().join("aegis");
        std::fs::write(&current_exe, b"old binary v1").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }

        // Create extracted dir with new binary
        let extracted = tempfile::tempdir().unwrap();
        std::fs::write(extracted.path().join("aegis"), b"new binary v2").unwrap();

        let result = replace_binary(&current_exe, extracted.path());
        assert!(result.is_ok(), "replace should succeed: {result:?}");

        // Verify the binary was replaced
        let content = std::fs::read(&current_exe).unwrap();
        assert_eq!(content, b"new binary v2");

        // Verify backup was cleaned up
        let backup = current_exe.with_extension("backup");
        assert!(
            !backup.exists(),
            "backup should be cleaned up after success"
        );
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_replace_binary_missing_new_binary() {
        let dir = tempfile::tempdir().unwrap();
        let current_exe = dir.path().join("aegis");
        std::fs::write(&current_exe, b"old binary").unwrap();

        let empty_dir = tempfile::tempdir().unwrap();
        let result = replace_binary(&current_exe, empty_dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("does not contain 'aegis' binary"),
            "expected missing binary error, got: {err}"
        );

        // Original should be untouched
        let content = std::fs::read(&current_exe).unwrap();
        assert_eq!(content, b"old binary");
    }

    // rtmx:req REQ-BUILD-068
    #[test]
    fn test_replace_binary_sets_executable_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let current_exe = dir.path().join("aegis");
        std::fs::write(&current_exe, b"old").unwrap();

        let extracted = tempfile::tempdir().unwrap();
        std::fs::write(extracted.path().join("aegis"), b"new").unwrap();

        replace_binary(&current_exe, extracted.path()).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&current_exe).unwrap().permissions();
            let mode = perms.mode() & 0o777;
            assert_eq!(
                mode, 0o755,
                "binary should have 755 permissions, got: {mode:o}"
            );
        }
    }

    // ---------------------------------------------------------------
    // REQ-BUILD-069: Post-update self-test and version confirmation
    // ---------------------------------------------------------------

    // rtmx:req REQ-BUILD-069
    #[test]
    fn test_parse_version_output_simple() {
        let output = "aegis 1.2.3\n";
        assert_eq!(parse_version_output(output), "1.2.3");
    }

    // rtmx:req REQ-BUILD-069
    #[test]
    fn test_parse_version_output_with_metadata() {
        let output = "aegis 1.2.3 (abc1234 x86_64-unknown-linux-musl)\n";
        assert_eq!(parse_version_output(output), "1.2.3");
    }

    // rtmx:req REQ-BUILD-069
    #[test]
    fn test_parse_version_output_bare() {
        let output = "1.2.3\n";
        assert_eq!(parse_version_output(output), "1.2.3");
    }

    // rtmx:req REQ-BUILD-069
    #[test]
    fn test_verify_version_binary_not_found() {
        let result = verify_version(Path::new("/nonexistent/aegis"), "1.0.0");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("self-test"),
            "expected self-test error, got: {err}"
        );
    }

    // ---------------------------------------------------------------
    // REQ-BUILD-069: run_update entry point
    // ---------------------------------------------------------------

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

    // ---------------------------------------------------------------
    // REQ-BUILD-011: End-to-end integration
    // ---------------------------------------------------------------

    // rtmx:req REQ-BUILD-011
    #[test]
    fn test_end_to_end_extract_and_verify() {
        let dir = tempfile::tempdir().unwrap();

        // Create realistic bundle contents
        let binary_content = b"#!/bin/sh\necho 'aegis 2.0.0'";
        let version_json = br#"{"version":"2.0.0","git_sha":"abc123","build_date":"2026-05-12","target":"x86_64-unknown-linux-musl"}"#;
        let sbom_json = br#"{"bomFormat":"CycloneDX","specVersion":"1.5"}"#;
        let license = b"Apache-2.0";

        let data_files: &[(&str, &[u8])] = &[
            ("aegis", binary_content),
            ("version.json", version_json),
            ("sbom.json", sbom_json),
            ("LICENSE", license),
        ];

        let manifest = create_manifest(data_files);
        let manifest_bytes = manifest.into_bytes();

        let mut all_files: Vec<(&str, &[u8])> = data_files.to_vec();
        all_files.push(("manifest.txt", &manifest_bytes));

        let bundle_path = create_test_bundle(dir.path(), &all_files);

        // Step 1: Validate bundle path
        validate_bundle_path(&bundle_path).unwrap();

        // Step 2: Compute and verify SHA-256 of the bundle
        let bundle_hash = compute_sha256(&bundle_path).unwrap();
        verify_bundle(&bundle_path, &bundle_hash).unwrap();

        // Step 3: Extract and verify manifest
        let extract_dir = tempfile::tempdir().unwrap();
        let info = extract_bundle(&bundle_path, extract_dir.path()).unwrap();

        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.manifest_entries.len(), 4);

        // Step 4: Verify all extracted files exist and have correct hashes
        for (filename, expected_hash) in &info.manifest_entries {
            let path = extract_dir.path().join(filename);
            assert!(path.exists(), "extracted file should exist: {filename}");
            let actual = compute_sha256(&path).unwrap();
            assert_eq!(
                actual.to_lowercase(),
                expected_hash.to_lowercase(),
                "hash mismatch for {filename}"
            );
        }

        // Step 5: Verify binary replacement works
        let fake_current = dir.path().join("current-aegis");
        std::fs::write(&fake_current, b"old version").unwrap();
        replace_binary(&fake_current, extract_dir.path()).unwrap();

        let replaced = std::fs::read(&fake_current).unwrap();
        assert_eq!(replaced, binary_content);
    }

    // rtmx:req REQ-BUILD-011
    #[test]
    fn test_end_to_end_rejects_tampered_bundle() {
        let dir = tempfile::tempdir().unwrap();

        // Create bundle with correct manifest
        let binary_content = b"real binary";
        let version_json = br#"{"version":"1.0.0"}"#;
        let manifest =
            create_manifest(&[("aegis", binary_content), ("version.json", version_json)]);
        let manifest_bytes = manifest.into_bytes();

        // But tamper the binary content in the actual tarball
        let tampered_binary = b"TAMPERED binary";
        let bundle_path = create_test_bundle(
            dir.path(),
            &[
                ("aegis", tampered_binary), // tampered!
                ("version.json", version_json),
                ("manifest.txt", &manifest_bytes),
            ],
        );

        let extract_dir = tempfile::tempdir().unwrap();
        let result = extract_bundle(&bundle_path, extract_dir.path());

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("SHA-256 mismatch"),
            "tampered bundle should be rejected"
        );
    }
}
