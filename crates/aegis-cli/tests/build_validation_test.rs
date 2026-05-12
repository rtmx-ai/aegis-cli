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

// ---------------------------------------------------------------------------
// REQ-BUILD-003: Binary signing and SBOM generation (parent rollup)
//
// Acceptance criterion: "Binary is code-signed with SBOM attached"
// Validates that all three sub-requirements (039 SBOM, 040 GPG, 041
// Authenticode) are wired into the release workflow such that every
// platform's artifacts are signed and Linux artifacts include an SBOM.
// ---------------------------------------------------------------------------

// rtmx:req REQ-BUILD-003
#[test]
fn test_every_release_platform_has_signing() {
    let release = read_release_yml();
    // Linux: GPG detached signatures on all artifacts
    assert!(
        release.contains("gpg") && release.contains("detach-sign"),
        "release workflow must GPG-sign Linux artifacts (REQ-BUILD-040)"
    );
    // Windows: Authenticode embedded signature
    assert!(
        release.contains("Set-AuthenticodeSignature"),
        "release workflow must Authenticode-sign Windows artifacts (REQ-BUILD-041)"
    );
    // Sign job depends on all platform builds
    assert!(
        release.contains("needs: [build-linux, build-linux-aarch64, build-macos, build-windows]"),
        "sign job must depend on all platform builds to sign everything"
    );
}

// rtmx:req REQ-BUILD-003
#[test]
fn test_sbom_generated_and_included_in_release() {
    let release = read_release_yml();
    // SBOM generated via cargo-cyclonedx in Linux build
    assert!(
        release.contains("cargo-cyclonedx") || release.contains("cargo cyclonedx"),
        "release workflow must generate CycloneDX SBOM"
    );
    // SBOM copied into release artifacts for signing
    assert!(
        release.contains("aegis-cli.cdx.json") && release.contains("release-artifacts"),
        "SBOM must be collected into release-artifacts for GPG signing"
    );
    // SBOM included in airgap bundle
    assert!(
        release.contains("sbom.json"),
        "SBOM must be included in airgap bundle as sbom.json"
    );
}

// rtmx:req REQ-BUILD-003
#[test]
fn test_signing_verification_runs_before_release() {
    let release = read_release_yml();
    // Sign job verifies every signature before upload
    assert!(
        release.contains("gpg --verify"),
        "sign job must verify all GPG signatures"
    );
    assert!(
        release.contains("Get-AuthenticodeSignature"),
        "Windows build must verify Authenticode signature"
    );
    // Release job depends on sign job (signatures verified before publish)
    assert!(
        release.contains("needs: [sign]"),
        "release job must depend on sign job -- no unsigned artifacts published"
    );
}

// ---------------------------------------------------------------------------
// REQ-BUILD-009: Windows MSI installer via WiX for enterprise push
// deployment (parent rollup)
//
// Acceptance criterion: "MSI installs silently via msiexec /qn"
// Validates the complete enterprise deployment chain: WiX source defines
// correct installer semantics, release workflow generates + signs + smoke
// tests the MSI, and the installer supports SCCM/Intune push deployment.
// ---------------------------------------------------------------------------

// rtmx:req REQ-BUILD-009
#[test]
fn test_msi_enterprise_deployment_chain() {
    let release = read_release_yml();
    // Full chain: generate -> smoke test -> sign -> publish
    assert!(
        release.contains("cargo wix"),
        "release must generate MSI via cargo wix (REQ-BUILD-047)"
    );
    assert!(
        release.contains("msiexec") && release.contains("/qn"),
        "release must smoke test silent install via msiexec /qn (REQ-BUILD-048)"
    );
    assert!(
        release.contains("Set-AuthenticodeSignature"),
        "release must Authenticode-sign the MSI (REQ-BUILD-041)"
    );
    // MSI uploaded as release artifact
    assert!(
        release.contains("*.msi") && release.contains("release-artifacts"),
        "signed MSI must be collected into release-artifacts"
    );
}

// rtmx:req REQ-BUILD-009
#[test]
fn test_wix_supports_enterprise_deployment() {
    let wxs_path = workspace_root().join("crates/aegis-cli/wix/main.wxs");
    let wxs = std::fs::read_to_string(&wxs_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", wxs_path.display()));
    // Per-machine install (required for SCCM push)
    assert!(
        wxs.contains("perMachine"),
        "MSI must use InstallScope='perMachine' for enterprise deployment"
    );
    // Stable UpgradeCode (required for Intune detection rules)
    assert!(
        wxs.contains("UpgradeCode"),
        "MSI must define a stable UpgradeCode for upgrade/detection"
    );
    // MajorUpgrade element (in-place upgrade support)
    assert!(
        wxs.contains("MajorUpgrade"),
        "MSI must define MajorUpgrade for in-place upgrades"
    );
    // Downgrade prevention
    assert!(
        wxs.contains("DowngradeErrorMessage"),
        "MSI must prevent downgrades with DowngradeErrorMessage"
    );
    // PATH integration (aegis available from cmd/powershell after install)
    assert!(
        wxs.contains("Environment") && wxs.contains("PATH"),
        "MSI must add install directory to system PATH"
    );
    // Installs to Program Files (not user profile)
    assert!(
        wxs.contains("ProgramFiles64Folder"),
        "MSI must install to 64-bit Program Files"
    );
}

// ---------------------------------------------------------------------------
// REQ-BUILD-010: Linux RPM/DEB with correct ownership and SELinux labels
// (parent rollup)
//
// Acceptance criterion: "RPM installs on RHEL 8/9 with correct SELinux type"
// Validates the complete Linux packaging chain: deb + rpm generation with
// correct file modes, SELinux context labeling, GPG signing, and smoke
// tests on real RHEL 9 containers.
// ---------------------------------------------------------------------------

// rtmx:req REQ-BUILD-010
#[test]
fn test_linux_packaging_chain() {
    let ci = read_ci_yml();
    let release = read_release_yml();
    // DEB generation (REQ-BUILD-042)
    assert!(
        ci.contains("cargo deb") || ci.contains("cargo-deb"),
        "CI must generate .deb package (REQ-BUILD-042)"
    );
    // RPM generation (REQ-BUILD-043)
    assert!(
        ci.contains("cargo generate-rpm") || ci.contains("cargo-generate-rpm"),
        "CI must generate .rpm package (REQ-BUILD-043)"
    );
    // Both packages generated in release workflow too
    assert!(
        release.contains("cargo deb") || release.contains("cargo-deb"),
        "release workflow must generate .deb package"
    );
    assert!(
        release.contains("cargo-generate-rpm"),
        "release workflow must generate .rpm package"
    );
    // GPG signing covers deb and rpm (REQ-BUILD-040)
    assert!(
        release.contains("*.deb") || release.contains("aegis-cli_"),
        "release must include .deb in signed artifacts"
    );
    assert!(
        release.contains("*.rpm") || release.contains("aegis-cli-"),
        "release must include .rpm in signed artifacts"
    );
}

// rtmx:req REQ-BUILD-010
#[test]
fn test_rpm_file_modes_and_selinux() {
    let toml =
        std::fs::read_to_string(workspace_root().join("crates/aegis-cli/Cargo.toml")).unwrap();
    // Binary installed with mode 755 (rwxr-xr-x)
    assert!(
        toml.contains("mode = \"755\"") && toml.contains("/usr/bin/aegis"),
        "RPM must install /usr/bin/aegis with mode 755"
    );
    // Documentation installed with mode 644 (rw-r--r--)
    assert!(
        toml.contains("mode = \"644\""),
        "RPM must install documentation with mode 644"
    );
    // SELinux post-install script (REQ-BUILD-044)
    assert!(
        toml.contains("semanage fcontext") && toml.contains("bin_t"),
        "RPM post-install must set SELinux type bin_t on /usr/bin/aegis"
    );
    assert!(
        toml.contains("restorecon"),
        "RPM post-install must run restorecon to apply SELinux context"
    );
}

// rtmx:req REQ-BUILD-010
#[test]
fn test_deb_file_modes() {
    let toml =
        std::fs::read_to_string(workspace_root().join("crates/aegis-cli/Cargo.toml")).unwrap();
    // Binary to /usr/bin/ with mode 755
    assert!(
        toml.contains("\"usr/bin/\", \"755\""),
        "DEB must install binary to /usr/bin/ with mode 755"
    );
    // License and README with mode 644
    assert!(
        toml.contains("\"644\""),
        "DEB must install docs with mode 644"
    );
}

// rtmx:req REQ-BUILD-010
#[test]
fn test_rpm_smoke_test_on_rhel9() {
    let ci = read_ci_yml();
    // Must test on real RHEL 9 image (not generic Ubuntu)
    assert!(
        ci.contains("redhat/ubi9"),
        "RPM smoke test must run in redhat/ubi9 container for RHEL 9 validation"
    );
    // Must install and run the binary
    assert!(
        ci.contains("rpm -i") && ci.contains("aegis --version"),
        "RPM smoke test must install via rpm and verify aegis --version"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_release_yml() -> String {
    let path = workspace_root().join(".github/workflows/release.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// rtmx:req REQ-BUILD-056
#[test]
fn test_msi_upgrade_code_is_not_placeholder() {
    let wxs = workspace_root().join("crates/aegis-cli/wix/main.wxs");
    let content =
        std::fs::read_to_string(&wxs).unwrap_or_else(|e| panic!("read {}: {e}", wxs.display()));
    assert!(
        !content.contains("F1A2B3C4-D5E6-7890-ABCD-EF1234567890"),
        "MSI UpgradeCode still contains placeholder GUID -- \
         generate a real UUID v4 before release"
    );
    // Verify it has *some* UpgradeCode that looks like a GUID
    assert!(
        content.contains("UpgradeCode='"),
        "WiX source must define an UpgradeCode"
    );
}

// rtmx:req REQ-BUILD-059
#[test]
fn test_changelog_exists_with_version_sections() {
    let changelog = workspace_root().join("CHANGELOG.md");
    assert!(
        changelog.exists(),
        "CHANGELOG.md must exist at workspace root"
    );
    let content = std::fs::read_to_string(&changelog).unwrap();
    assert!(
        content.contains("## [Unreleased]"),
        "CHANGELOG must have Unreleased section"
    );
    assert!(
        content.contains("## [0.1.0]"),
        "CHANGELOG must have 0.1.0 section"
    );
}

// rtmx:req REQ-BUILD-060
#[test]
fn test_deny_toml_covers_all_release_targets() {
    let deny = workspace_root().join("deny.toml");
    let content = std::fs::read_to_string(&deny).unwrap();
    for target in &[
        "x86_64-unknown-linux-musl",
        "x86_64-pc-windows-msvc",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-musl",
    ] {
        assert!(
            content.contains(target),
            "deny.toml must include target {target}"
        );
    }
}

fn read_ci_yml() -> String {
    let path = workspace_root().join(".github/workflows/ci.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
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

// rtmx:req REQ-BUILD-013
#[test]
fn test_ci_has_sccache_configuration() {
    let ci = workspace_root().join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("SCCACHE_GHA_ENABLED"),
        "CI must configure sccache for compilation caching"
    );
    assert!(
        content.contains("RUSTC_WRAPPER"),
        "CI must set RUSTC_WRAPPER for sccache"
    );
    assert!(
        content.contains("sccache-action"),
        "CI must use mozilla-actions/sccache-action"
    );
}

// rtmx:req REQ-TEST-033
#[test]
fn test_coverage_delta_script_exists() {
    let script = workspace_root().join("scripts/coverage-delta.sh");
    assert!(script.exists(), "scripts/coverage-delta.sh must exist");
    let content = std::fs::read_to_string(&script).unwrap();
    assert!(
        content.contains("gh pr comment"),
        "coverage delta script must post PR comments via gh"
    );
}

// rtmx:req REQ-TEST-036
#[test]
fn test_bdd_lint_script_exists() {
    let script = workspace_root().join("scripts/bdd-lint.sh");
    assert!(script.exists(), "scripts/bdd-lint.sh must exist");
    let content = std::fs::read_to_string(&script).unwrap();
    assert!(
        content.contains("MISSING_GIVEN_WHEN_THEN"),
        "BDD linter must check for Given-When-Then structure"
    );
}

// rtmx:req REQ-TEST-034
#[test]
fn test_ci_has_coverage_threshold_gate() {
    let ci = workspace_root().join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("COVERAGE_FAIL_THRESHOLD"),
        "CI must define coverage fail threshold"
    );
    assert!(
        content.contains("COVERAGE_WARN_THRESHOLD"),
        "CI must define coverage warn threshold"
    );
    assert!(
        content.contains("cargo llvm-cov"),
        "CI must run cargo llvm-cov for coverage"
    );
}

// rtmx:req REQ-TEST-037
#[test]
fn test_bdd_step_reuse_audit_in_lint() {
    let script = workspace_root().join("scripts/bdd-lint.sh");
    assert!(script.exists(), "scripts/bdd-lint.sh must exist");
    let content = std::fs::read_to_string(&script).unwrap();
    assert!(
        content.contains("DUPLICATE") || content.contains("reuse") || content.contains("similar"),
        "BDD linter should audit step definition reuse"
    );
}

// ---------------------------------------------------------------------------
// REQ-BUILD-070: aarch64-unknown-linux-musl cross-compilation target in CI
// ---------------------------------------------------------------------------

// rtmx:req REQ-BUILD-070
#[test]
fn test_ci_has_aarch64_linux_target() {
    let ci = read_ci_yml();
    assert!(
        ci.contains("aarch64-unknown-linux-musl"),
        "CI binary-build matrix must include aarch64-unknown-linux-musl target"
    );
    assert!(
        ci.contains("gcc-aarch64-linux-gnu"),
        "CI must install gcc-aarch64-linux-gnu for aarch64 cross-compilation"
    );
    // Verify toolchain config exists
    let toolchain = workspace_root().join("rust-toolchain.toml");
    let toolchain_content = std::fs::read_to_string(&toolchain).unwrap();
    assert!(
        toolchain_content.contains("aarch64-unknown-linux-musl"),
        "rust-toolchain.toml must include aarch64-unknown-linux-musl target"
    );
    // Verify cargo config has linker setting
    let cargo_config = workspace_root().join(".cargo/config.toml");
    let cargo_content = std::fs::read_to_string(&cargo_config).unwrap();
    assert!(
        cargo_content.contains("[target.aarch64-unknown-linux-musl]"),
        ".cargo/config.toml must configure aarch64-unknown-linux-musl linker"
    );
}

// ---------------------------------------------------------------------------
// REQ-BUILD-071: Release workflow matrix includes aarch64 target
// ---------------------------------------------------------------------------

// rtmx:req REQ-BUILD-071
#[test]
fn test_release_has_aarch64_linux_build() {
    let release = read_release_yml();
    assert!(
        release.contains("aarch64-unknown-linux-musl"),
        "release workflow must build for aarch64-unknown-linux-musl"
    );
    assert!(
        release.contains("linux-aarch64"),
        "release workflow must produce linux-aarch64 tarball"
    );
    assert!(
        release.contains("build-linux-aarch64"),
        "release workflow must have a build-linux-aarch64 job"
    );
}

// ---------------------------------------------------------------------------
// REQ-BUILD-072: Smoke test aarch64 binary with qemu-user-static
// ---------------------------------------------------------------------------

// rtmx:req REQ-BUILD-072
#[test]
fn test_ci_has_aarch64_smoke_test() {
    let ci = read_ci_yml();
    assert!(
        ci.contains("qemu-user-static"),
        "CI must install qemu-user-static for aarch64 smoke testing"
    );
    assert!(
        ci.contains("qemu-aarch64-static"),
        "CI must run aarch64 binary via qemu-aarch64-static"
    );
}

// ---------------------------------------------------------------------------
// REQ-BUILD-073: Release asset upload for aarch64 binary
// ---------------------------------------------------------------------------

// rtmx:req REQ-BUILD-073
#[test]
fn test_release_has_aarch64_asset_upload() {
    let release = read_release_yml();
    // aarch64 artifacts must be included in the sign job
    assert!(
        release.contains("release-artifacts-linux-aarch64"),
        "release sign job must download aarch64 Linux artifacts"
    );
    // Sign job must depend on the aarch64 build
    assert!(
        release.contains("build-linux-aarch64"),
        "release sign job must depend on build-linux-aarch64"
    );
    // aarch64 tarball is created
    assert!(
        release.contains("linux-aarch64.tar.gz"),
        "release must create linux-aarch64 tarball"
    );
}

// rtmx:req REQ-BUILD-061
#[test]
fn test_cargo_config_toml_exists() {
    let config = workspace_root().join(".cargo/config.toml");
    assert!(
        config.exists(),
        ".cargo/config.toml must exist for build acceleration"
    );
    let content = std::fs::read_to_string(&config).unwrap();
    assert!(
        content.contains("split-debuginfo"),
        ".cargo/config.toml must configure split-debuginfo for macOS"
    );
}
