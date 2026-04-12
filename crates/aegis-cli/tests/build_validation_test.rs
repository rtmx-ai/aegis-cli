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

// rtmx:req REQ-BUILD-024
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

// rtmx:req REQ-BUILD-026
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

// rtmx:req REQ-BUILD-026
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

// rtmx:req REQ-BUILD-040
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

// rtmx:req REQ-BUILD-041
#[test]
fn test_release_workflow_has_authenticode_signing() {
    let release_yml = workspace_root().join(".github/workflows/release.yml");
    let content = std::fs::read_to_string(&release_yml).unwrap();
    assert!(
        content.contains("New-SelfSignedCertificate"),
        "release workflow must generate a self-signed Authenticode certificate"
    );
    assert!(
        content.contains("Set-AuthenticodeSignature"),
        "release workflow must sign Windows binary with Authenticode"
    );
    assert!(
        content.contains("Get-AuthenticodeSignature"),
        "release workflow must verify the Authenticode signature"
    );
    assert!(
        content.contains("windows-latest"),
        "release workflow must build on windows-latest"
    );
    assert!(
        content.contains("x86_64-pc-windows-msvc"),
        "release workflow must target MSVC"
    );
}

// rtmx:req REQ-BUILD-047
#[test]
fn test_wix_source_file_exists() {
    let wxs = workspace_root().join("crates/aegis-cli/wix/main.wxs");
    assert!(
        wxs.exists(),
        "WiX source file must exist at crates/aegis-cli/wix/main.wxs"
    );
    let content = std::fs::read_to_string(&wxs).unwrap();
    assert!(
        content.contains("aegis.exe"),
        "WiX source must reference aegis.exe"
    );
    assert!(
        content.contains("UpgradeCode"),
        "WiX source must define UpgradeCode for upgrades"
    );
    assert!(
        content.contains("perMachine"),
        "WiX source must install per-machine"
    );
    assert!(
        content.contains("PATH"),
        "WiX source must add aegis to PATH"
    );
}

// rtmx:req REQ-BUILD-047
#[test]
fn test_release_workflow_has_wix_msi_generation() {
    let release_yml = workspace_root().join(".github/workflows/release.yml");
    let content = std::fs::read_to_string(&release_yml).unwrap();
    assert!(
        content.contains("cargo-wix"),
        "release workflow must install cargo-wix"
    );
    assert!(
        content.contains("cargo wix"),
        "release workflow must run cargo wix to generate MSI"
    );
}

// rtmx:req REQ-BUILD-048
#[test]
fn test_release_workflow_has_msi_smoke_test() {
    let release_yml = workspace_root().join(".github/workflows/release.yml");
    let content = std::fs::read_to_string(&release_yml).unwrap();
    assert!(
        content.contains("msiexec"),
        "release workflow must run msiexec for silent install smoke test"
    );
    assert!(
        content.contains("aegis --version"),
        "release workflow must verify aegis runs after MSI install"
    );
}

// rtmx:req REQ-BUILD-005
#[test]
fn test_rust_toolchain_file_exists() {
    let toolchain = workspace_root().join("rust-toolchain.toml");
    assert!(
        toolchain.exists(),
        "rust-toolchain.toml must exist for reproducible builds"
    );
    let content = std::fs::read_to_string(&toolchain).unwrap();
    assert!(
        content.contains("channel"),
        "rust-toolchain.toml must specify a channel"
    );
    // Must be a pinned version, not "stable" or "nightly"
    assert!(
        !content.contains("channel = \"stable\"") && !content.contains("channel = \"nightly\""),
        "rust-toolchain.toml must pin a specific version, not stable/nightly"
    );
}

// rtmx:req REQ-BUILD-005
#[test]
fn test_cargo_lock_is_committed() {
    let lock = workspace_root().join("Cargo.lock");
    assert!(lock.exists(), "Cargo.lock must exist and be committed");
}

// rtmx:req REQ-BUILD-005
#[test]
fn test_ci_sets_source_date_epoch() {
    let ci = workspace_root().join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("SOURCE_DATE_EPOCH"),
        "CI must set SOURCE_DATE_EPOCH for reproducible builds"
    );
}

// rtmx:req REQ-BUILD-005
#[test]
fn test_release_sets_source_date_epoch() {
    let release = workspace_root().join(".github/workflows/release.yml");
    let content = std::fs::read_to_string(&release).unwrap();
    assert!(
        content.contains("SOURCE_DATE_EPOCH"),
        "release workflow must set SOURCE_DATE_EPOCH for reproducible builds"
    );
}

// rtmx:req REQ-BUILD-027
#[test]
fn test_homebrew_formula_exists() {
    let formula = workspace_root().join("packaging/homebrew/aegis.rb");
    assert!(formula.exists(), "packaging/homebrew/aegis.rb must exist");
    let content = std::fs::read_to_string(&formula).unwrap();
    assert!(content.contains("on_arm"), "must support ARM");
    assert!(content.contains("on_intel"), "must support Intel");
    assert!(content.contains("Apache-2.0"), "must specify license");
}

// rtmx:req REQ-BUILD-027
#[test]
fn test_homebrew_formula_has_both_arch_urls() {
    let formula = workspace_root().join("packaging/homebrew/aegis.rb");
    let content = std::fs::read_to_string(&formula).unwrap();
    assert!(
        content.contains("darwin-aarch64"),
        "formula must reference darwin-aarch64 tarball"
    );
    assert!(
        content.contains("darwin-x86_64"),
        "formula must reference darwin-x86_64 tarball"
    );
    assert!(
        content.contains("on_macos"),
        "formula must use on_macos block"
    );
}
