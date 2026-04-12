//! Validates BDD scenario execution coverage report (REQ-TEST-031).

// rtmx:req REQ-TEST-031
#[test]
fn test_bdd_coverage_script_exists() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/bdd-coverage-report.sh");
    assert!(script.exists(), "bdd-coverage-report.sh must exist");
}

// rtmx:req REQ-TEST-031
#[test]
fn test_bdd_coverage_script_is_executable() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts/bdd-coverage-report.sh");
        let perms = std::fs::metadata(&script).unwrap().permissions();
        assert!(
            perms.mode() & 0o111 != 0,
            "bdd-coverage-report.sh must be executable"
        );
    }
}
