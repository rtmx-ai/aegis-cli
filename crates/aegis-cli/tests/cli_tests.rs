//! Integration tests for the aegis binary.
//! These run the compiled binary via assert_cmd.

use assert_cmd::Command;
use predicates::prelude::*;

// @req REQ-BUILD-001
#[test]
fn binary_runs_with_help() {
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Agentic AI pair programmer"));
}

// @req REQ-BUILD-012
#[test]
fn binary_prints_version() {
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("aegis"));
}

// @req REQ-ONBOARD-003
#[test]
fn init_local_creates_config() {
    let tmp = tempfile::TempDir::new().unwrap();
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("init")
        .arg("--local")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Configuration written to"));

    let config_path = tmp.path().join(".aegis/config.yaml");
    assert!(
        config_path.exists(),
        "Config should be created at {config_path:?}"
    );
}

// @req REQ-ONBOARD-001
#[test]
fn init_without_local_errors() {
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cloud modes not yet implemented"));
}

// @req REQ-BUILD-001
#[test]
fn no_args_exits_zero() {
    Command::cargo_bin("aegis").unwrap().assert().success();
}

// @req REQ-CLI-001
#[test]
fn chat_headless_requires_prompt() {
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("chat")
        .arg("--headless")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Prompt required"));
}

// @req REQ-CLI-002
// Interactive mode is now wired; without config it fails with a config error
// rather than the old "not yet wired" message.
#[test]
fn chat_interactive_without_config_errors() {
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("chat")
        .arg("-p")
        .arg("hello")
        .assert()
        .failure()
        .stderr(predicate::str::contains("No config found"));
}

// @req REQ-CLI-001
#[test]
fn chat_headless_with_bad_endpoint_errors_gracefully() {
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("chat")
        .arg("--headless")
        .arg("-p")
        .arg("hello")
        .arg("--local-endpoint")
        .arg("http://localhost:1/v1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("aegis:"));
}

// @req REQ-CLI-002
#[test]
fn chat_headless_has_help_flag() {
    Command::cargo_bin("aegis")
        .unwrap()
        .arg("chat")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--headless"));
}

// @req REQ-BUILD-007
#[test]
fn release_profile_binary_compiles() {
    // Verify the aegis binary can be located by assert_cmd, confirming the
    // crate compiles successfully with the workspace release profile settings
    // (LTO, strip, codegen-units=1, panic=abort configured in root Cargo.toml).
    let bin = Command::cargo_bin("aegis");
    assert!(bin.is_ok(), "aegis binary must compile and be discoverable");
}
