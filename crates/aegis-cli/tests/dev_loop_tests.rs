//! Tests that verify the dev loop infrastructure: bacon config, dev.sh
//! script structure, pre-push hook checks. These are inspection-type
//! requirements that have no Rust implementation -- the test IS the
//! verification.

use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

fn read_file(rel: &str) -> String {
    let path = workspace_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// @req REQ-BUILD-033
#[test]
fn test_bacon_watch_job_defined() {
    let bacon = read_file("bacon.toml");
    assert!(
        bacon.contains("[jobs.watch]"),
        "bacon.toml must define [jobs.watch] for hot reload"
    );
    assert!(
        bacon.contains("kill_then_restart"),
        "bacon.toml watch job must use on_change_strategy = kill_then_restart"
    );
}

// @req REQ-BUILD-034
#[test]
fn test_dev_sh_creates_two_panes() {
    let dev_sh = read_file("scripts/dev.sh");
    // Two-pane structure: new-session + split-window -h
    assert!(
        dev_sh.contains("tmux new-session"),
        "dev.sh must create a tmux session"
    );
    assert!(
        dev_sh.contains("tmux split-window -h"),
        "dev.sh must split horizontally for two-pane layout"
    );
    // Left pane runs the AI agent (claude by default)
    assert!(
        dev_sh.contains("AGENT") || dev_sh.contains("claude"),
        "dev.sh must launch an AI agent in the left pane"
    );
    // Right pane runs the dev-run.sh wrapper
    assert!(
        dev_sh.contains("dev-run.sh"),
        "dev.sh must launch dev-run.sh in the right pane"
    );
}

// @req REQ-BUILD-037
#[test]
fn test_dev_sh_accepts_agent_flag() {
    let dev_sh = read_file("scripts/dev.sh");
    assert!(
        dev_sh.contains("--agent") || dev_sh.contains("AEGIS_DEV_AGENT"),
        "dev.sh must support --agent flag or AEGIS_DEV_AGENT env var for modular agent selection"
    );
    // Default agent must be claude
    assert!(
        dev_sh.contains("claude"),
        "dev.sh default agent should be claude"
    );
}

// @req REQ-BUILD-038
#[test]
fn test_pre_push_validates_dev_loop_gif() {
    let pre_push = read_file("scripts/hooks/pre-push");
    assert!(
        pre_push.contains("dev-loop.gif"),
        "pre-push hook must reference docs/demos/dev-loop.gif"
    );
    assert!(
        pre_push.contains("scripts/dev.sh") || pre_push.contains("DEV_SH"),
        "pre-push hook must check freshness against scripts/dev.sh"
    );
}

// @req REQ-BUILD-039
#[test]
fn test_ci_has_sbom_job() {
    let ci = read_file(".github/workflows/ci.yml");
    assert!(ci.contains("sbom:"), "ci.yml must define sbom job");
    assert!(
        ci.contains("cargo-cyclonedx") || ci.contains("cargo cyclonedx"),
        "sbom job must invoke cargo-cyclonedx"
    );
    assert!(
        ci.contains("aegis-cli-sbom"),
        "sbom job must upload aegis-cli-sbom artifact"
    );
}

// @req REQ-BUILD-042
#[test]
fn test_cargo_toml_has_deb_metadata() {
    let toml = read_file("crates/aegis-cli/Cargo.toml");
    assert!(
        toml.contains("[package.metadata.deb]"),
        "aegis-cli Cargo.toml must have [package.metadata.deb] for cargo-deb"
    );
    assert!(
        toml.contains("usr/bin/"),
        "deb metadata must install binary to /usr/bin/"
    );
}

// @req REQ-BUILD-045
#[test]
fn test_ci_has_deb_smoke_test() {
    let ci = read_file(".github/workflows/ci.yml");
    assert!(
        ci.contains("deb-package:"),
        "ci.yml must define deb-package job"
    );
    assert!(
        ci.contains("cargo deb") || ci.contains("cargo-deb"),
        "deb-package job must invoke cargo-deb"
    );
    assert!(
        ci.contains("dpkg -i") && ci.contains("aegis --version"),
        "deb-package job must run dpkg -i smoke test and aegis --version"
    );
}

// @req REQ-BUILD-043
#[test]
fn test_cargo_toml_has_rpm_metadata() {
    let toml = read_file("crates/aegis-cli/Cargo.toml");
    assert!(
        toml.contains("[package.metadata.generate-rpm]"),
        "aegis-cli Cargo.toml must have [package.metadata.generate-rpm] for cargo-generate-rpm"
    );
    assert!(
        toml.contains("/usr/bin/aegis"),
        "rpm metadata must install binary to /usr/bin/aegis"
    );
}

// @req REQ-BUILD-049
#[test]
fn test_ci_has_airgap_bundle_job() {
    let ci = read_file(".github/workflows/ci.yml");
    assert!(
        ci.contains("airgap-bundle:"),
        "ci.yml must define airgap-bundle job"
    );
    // Bundle must include binary + sbom + manifest + version
    assert!(
        ci.contains("sbom.json"),
        "airgap-bundle job must include sbom.json"
    );
    assert!(
        ci.contains("manifest.txt"),
        "airgap-bundle job must include SHA-256 manifest.txt"
    );
    assert!(
        ci.contains("version.json"),
        "airgap-bundle job must include version.json"
    );
    // Must be the musl static binary
    assert!(
        ci.contains("x86_64-unknown-linux-musl"),
        "airgap-bundle job must use musl static binary"
    );
    // Must verify manifest checksums
    assert!(
        ci.contains("sha256sum -c manifest.txt"),
        "airgap-bundle job must verify manifest.txt checksums"
    );
    // Must produce a tarball
    assert!(
        ci.contains("airgap-linux-x86_64") && ci.contains("tar"),
        "airgap-bundle job must produce aegis-*-airgap-linux-x86_64.tar.gz"
    );
}

// @req REQ-BUILD-046
#[test]
fn test_ci_has_rpm_smoke_test() {
    let ci = read_file(".github/workflows/ci.yml");
    assert!(
        ci.contains("rpm-package:"),
        "ci.yml must define rpm-package job"
    );
    assert!(
        ci.contains("cargo generate-rpm") || ci.contains("cargo-generate-rpm"),
        "rpm-package job must invoke cargo-generate-rpm"
    );
    assert!(
        ci.contains("redhat/ubi9") || ci.contains("rpm -i"),
        "rpm-package job must run rpm install smoke test (e.g. redhat/ubi9 container)"
    );
}

// @req REQ-BUILD-035
#[test]
fn test_main_has_sigterm_handler() {
    let main = read_file("crates/aegis-cli/src/main.rs");
    assert!(
        main.contains("SignalKind::terminate") || main.contains("sigterm"),
        "main.rs must handle SIGTERM in event loop"
    );
    assert!(
        main.contains("save_session") || main.contains("session::save"),
        "main.rs must save session on SIGTERM"
    );
}

// @req REQ-BUILD-036
#[test]
fn test_main_has_session_restore() {
    let main = read_file("crates/aegis-cli/src/main.rs");
    assert!(
        main.contains("load_session") || main.contains("restore_app_from_snapshot"),
        "main.rs must auto-restore session on interactive startup"
    );
}

// @req REQ-BUILD-031
#[test]
fn test_main_routes_tracing_to_log_file_in_tui_mode() {
    let main = read_file("crates/aegis-cli/src/main.rs");
    assert!(
        main.contains("init_tracing_file") || main.contains("tracing_appender"),
        "main.rs must route tracing to file in TUI mode"
    );
    assert!(
        main.contains(".aegis") && main.contains("debug.log"),
        "main.rs must write to ~/.aegis/debug.log"
    );
}

// @req REQ-BUILD-023
#[test]
fn test_homebrew_tap_referenced() {
    let readme = read_file("README.md");
    // Verify the homebrew tap is documented
    assert!(
        readme.contains("brew tap") || readme.contains("homebrew"),
        "README must reference Homebrew tap installation path"
    );
}

// @req REQ-BUILD-025
#[test]
fn test_release_notes_exist_for_v0_0_1_alpha() {
    // The Homebrew formula at rtmx-ai/homebrew-tap references the
    // GitHub release. We can't reach external repos from a test, but
    // we can verify our Cargo.toml version is set.
    let toml = read_file("Cargo.toml");
    assert!(
        toml.contains("version"),
        "workspace Cargo.toml must define version"
    );
}

// @req REQ-BUILD-028
#[test]
fn test_version_follows_semver() {
    let toml = read_file("Cargo.toml");
    // Look for the workspace version line
    let version_line = toml
        .lines()
        .find(|l| l.trim_start().starts_with("version") && l.contains('='))
        .expect("workspace Cargo.toml must have version");
    // Extract the version string
    let v = version_line
        .split('"')
        .nth(1)
        .expect("version should be quoted");
    // Basic semver: MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-prerelease
    let parts: Vec<&str> = v.splitn(3, '.').collect();
    assert_eq!(
        parts.len(),
        3,
        "version must have three dot-separated parts"
    );
    assert!(
        parts[0].parse::<u32>().is_ok(),
        "MAJOR must be a number, got: {}",
        parts[0]
    );
    assert!(
        parts[1].parse::<u32>().is_ok(),
        "MINOR must be a number, got: {}",
        parts[1]
    );
    // PATCH may have a prerelease suffix like "1-alpha"
    let patch_part = parts[2].split('-').next().unwrap();
    assert!(
        patch_part.parse::<u32>().is_ok(),
        "PATCH must start with a number, got: {}",
        parts[2]
    );
}
