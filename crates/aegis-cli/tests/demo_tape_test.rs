//! Inspection tests for VHS demo tape scripts.
//!
//! Verifies that all demo tapes exist, contain valid VHS directives,
//! and that CI/README infrastructure is wired for GIF generation.

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

/// Verify a tape file has required VHS directives: Output, Set, Type.
fn assert_tape_parses(rel_path: &str) {
    let content = read_file(rel_path);
    assert!(
        content.contains("Output "),
        "{rel_path}: tape must have an Output directive"
    );
    assert!(
        content.contains("Set Width") && content.contains("Set Height"),
        "{rel_path}: tape must set Width and Height"
    );
    assert!(
        content.contains("Set FontSize"),
        "{rel_path}: tape must set FontSize"
    );
    assert!(
        content.contains("Type "),
        "{rel_path}: tape must have at least one Type directive"
    );
    assert!(
        content.contains("Sleep "),
        "{rel_path}: tape must have at least one Sleep directive"
    );
}

// rtmx:req REQ-BUILD-017
#[test]
fn test_hero_tape_parses() {
    let content = read_file("docs/demos/tapes/hero.tape");
    assert_tape_parses("docs/demos/tapes/hero.tape");
    // Hero must show full REA loop: chat, tool call, HITL approval
    assert!(
        content.contains("aegis chat"),
        "hero tape must launch aegis chat"
    );
    assert!(
        content.contains("rtmx:req REQ-BUILD-017"),
        "hero tape must have REQ-BUILD-017 marker"
    );
}

// rtmx:req REQ-BUILD-018
#[test]
fn test_hitl_tape_parses() {
    let content = read_file("docs/demos/tapes/hitl-approval.tape");
    assert_tape_parses("docs/demos/tapes/hitl-approval.tape");
    // HITL tape must show deny then approve flow
    assert!(
        content.contains("\"n\"") || content.contains("Type \"n"),
        "HITL tape must show user denying a proposal"
    );
    assert!(
        content.contains("\"y\"") || content.contains("Type \"y"),
        "HITL tape must show user approving a proposal"
    );
    assert!(
        content.contains("rtmx:req REQ-BUILD-018"),
        "HITL tape must have REQ-BUILD-018 marker"
    );
}

// rtmx:req REQ-BUILD-019
#[test]
fn test_airgapped_tape_parses() {
    let content = read_file("docs/demos/tapes/airgapped.tape");
    assert_tape_parses("docs/demos/tapes/airgapped.tape");
    // Air-gapped tape must show local/offline mode
    assert!(
        content.contains("--local") || content.contains("air-gapped"),
        "air-gapped tape must demonstrate local/offline mode"
    );
    assert!(
        content.contains("rtmx:req REQ-BUILD-019"),
        "air-gapped tape must have REQ-BUILD-019 marker"
    );
}

// rtmx:req REQ-BUILD-020
#[test]
fn test_audit_tape_parses() {
    let content = read_file("docs/demos/tapes/audit-ledger.tape");
    assert_tape_parses("docs/demos/tapes/audit-ledger.tape");
    // Audit tape must show ledger inspection and hash chain verification
    assert!(
        content.contains("audit") || content.contains("ledger"),
        "audit tape must demonstrate ledger inspection"
    );
    assert!(
        content.contains("rtmx:req REQ-BUILD-020"),
        "audit tape must have REQ-BUILD-020 marker"
    );
}

// rtmx:req REQ-BUILD-021
#[test]
fn test_plugin_tape_parses() {
    let content = read_file("docs/demos/tapes/plugin-provision.tape");
    assert_tape_parses("docs/demos/tapes/plugin-provision.tape");
    // Plugin tape must show plugin lifecycle with progress events
    assert!(
        content.contains("aegis init") || content.contains("plugin"),
        "plugin tape must demonstrate plugin provisioning"
    );
    assert!(
        content.contains("rtmx:req REQ-BUILD-021"),
        "plugin tape must have REQ-BUILD-021 marker"
    );
}

// rtmx:req REQ-BUILD-022
#[test]
fn test_aegisignore_tape_parses() {
    let content = read_file("docs/demos/tapes/aegisignore.tape");
    assert_tape_parses("docs/demos/tapes/aegisignore.tape");
    // Aegisignore tape must show .aegisignore blocking file access
    assert!(
        content.contains(".aegisignore") || content.contains(".env"),
        "aegisignore tape must demonstrate context filtering"
    );
    assert!(
        content.contains("rtmx:req REQ-BUILD-022"),
        "aegisignore tape must have REQ-BUILD-022 marker"
    );
}

// rtmx:req REQ-BUILD-014
#[test]
fn test_demo_tapes_parse() {
    // All 6 numbered demo tapes must exist and parse as valid VHS scripts.
    let tapes = [
        "docs/demos/tapes/01-hero.tape",
        "docs/demos/tapes/02-hitl-approval.tape",
        "docs/demos/tapes/03-airgapped.tape",
        "docs/demos/tapes/04-audit-ledger.tape",
        "docs/demos/tapes/05-plugin-provision.tape",
        "docs/demos/tapes/06-aegisignore.tape",
    ];
    for tape in &tapes {
        assert_tape_parses(tape);
    }
    // Also verify the named tapes used by individual requirements
    assert_tape_parses("docs/demos/tapes/hero.tape");
    assert_tape_parses("docs/demos/tapes/hitl-approval.tape");
    assert_tape_parses("docs/demos/tapes/airgapped.tape");
}

// rtmx:req REQ-BUILD-015
#[test]
fn test_ci_gif_generation() {
    let ci = read_file(".github/workflows/ci.yml");
    assert!(
        ci.contains("demo-gifs:") || ci.contains("demo-gifs"),
        "ci.yml must define demo-gifs job"
    );
    assert!(
        ci.contains("vhs") || ci.contains("charmbracelet/vhs"),
        "demo-gifs job must use vhs for GIF generation"
    );
    assert!(
        ci.contains(".tape"),
        "demo-gifs job must reference .tape files"
    );
}

// rtmx:req REQ-BUILD-016
#[test]
fn test_readme_gif_links_valid() {
    let readme = read_file("README.md");
    // Hero GIF at top of README
    assert!(
        readme.contains("docs/demos/") && readme.contains(".gif"),
        "README must embed demo GIFs from docs/demos/"
    );
    assert!(
        readme.contains("hero"),
        "README must reference the hero demo GIF"
    );
}
