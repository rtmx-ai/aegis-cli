// Validation tests for BUILD requirements that verify CI/CD workflow structure.
// These tests read workflow YAML files and assert required configuration is present.

/// Helper: resolve workspace root from CARGO_MANIFEST_DIR (crates/aegis-cli -> repo root).
fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

// @req REQ-BUILD-024
#[test]
fn test_release_workflow_exists_and_creates_github_release() {
    let release_yml = workspace_root().join(".github/workflows/release.yml");
    assert!(release_yml.exists(), "release.yml must exist");
    let content = std::fs::read_to_string(&release_yml).unwrap();
    assert!(
        content.contains("tags:"),
        "release workflow must trigger on tags"
    );
    assert!(
        content.contains("gh release create") || content.contains("action-gh-release"),
        "release workflow must create a GitHub release"
    );
    assert!(
        content.contains("generate-notes"),
        "release workflow must generate release notes"
    );
    assert!(
        content.contains("upload-artifact") || content.contains("artifacts/*"),
        "release workflow must upload artifacts to the release"
    );
}

// @req REQ-BUILD-026
#[test]
fn test_ci_has_macos_aarch64_build() {
    let ci = workspace_root().join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("aarch64-apple-darwin"),
        "CI must include aarch64-apple-darwin target for macOS Apple Silicon"
    );
    assert!(
        content.contains("macos-latest"),
        "CI must use macos-latest runner for Apple Silicon builds"
    );
}

// @req REQ-BUILD-026
#[test]
fn test_release_has_macos_aarch64_build() {
    let release_yml = workspace_root().join(".github/workflows/release.yml");
    let content = std::fs::read_to_string(&release_yml).unwrap();
    assert!(
        content.contains("aarch64-apple-darwin"),
        "release workflow must build for macOS aarch64"
    );
    assert!(
        content.contains("macos-aarch64"),
        "release workflow must produce macOS aarch64 tarball"
    );
}

// @req REQ-BUILD-040
#[test]
fn test_release_workflow_has_gpg_signing() {
    let release_yml = workspace_root().join(".github/workflows/release.yml");
    let content = std::fs::read_to_string(&release_yml).unwrap();
    assert!(
        content.contains("GPG_PRIVATE_KEY"),
        "release workflow must reference GPG_PRIVATE_KEY secret"
    );
    assert!(
        content.contains("detach-sign"),
        "release workflow must use gpg --detach-sign for artifact signing"
    );
    assert!(
        content.contains("gpg --verify"),
        "release workflow must verify GPG signatures"
    );
}
