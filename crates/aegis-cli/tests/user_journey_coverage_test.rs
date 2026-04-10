//! Validates user journey coverage metric infrastructure (REQ-TEST-032).

// @req REQ-TEST-032
#[test]
fn test_user_journey_coverage_script_exists() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/user-journey-coverage.py");
    assert!(script.exists(), "user-journey-coverage.py must exist");
}

// @req REQ-TEST-032
#[test]
fn test_user_journey_coverage_script_is_executable() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts/user-journey-coverage.py");
        let perms = std::fs::metadata(&script).unwrap().permissions();
        assert!(
            perms.mode() & 0o111 != 0,
            "user-journey-coverage.py must be executable"
        );
    }
}
