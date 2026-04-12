//! Validates test failure downgrade automation (REQ-TEST-041).

// rtmx:req REQ-TEST-041
#[test]
fn test_failure_downgrade_script_exists() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/test-failure-downgrade.py");
    assert!(script.exists(), "test-failure-downgrade.py must exist");
}

// rtmx:req REQ-TEST-041
#[test]
fn test_failure_downgrade_script_is_executable() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts/test-failure-downgrade.py");
        let perms = std::fs::metadata(&script).unwrap().permissions();
        assert!(
            perms.mode() & 0o111 != 0,
            "test-failure-downgrade.py must be executable"
        );
    }
}

// rtmx:req REQ-TEST-041
#[test]
fn test_failure_downgrade_script_has_dry_run() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/test-failure-downgrade.py");
    let content = std::fs::read_to_string(&script).unwrap();
    assert!(
        content.contains("--dry-run"),
        "test-failure-downgrade.py must support --dry-run flag"
    );
}
