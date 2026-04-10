//! Validates BDD step definition coverage infrastructure (REQ-TEST-008).

// @req REQ-TEST-008
#[test]
fn test_bdd_step_coverage_script_exists() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/bdd-step-coverage.py");
    assert!(script.exists(), "bdd-step-coverage.py must exist");
}

// @req REQ-TEST-008
#[test]
fn test_bdd_step_coverage_script_is_executable() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts/bdd-step-coverage.py");
        let perms = std::fs::metadata(&script).unwrap().permissions();
        assert!(
            perms.mode() & 0o111 != 0,
            "bdd-step-coverage.py must be executable"
        );
    }
}

// @req REQ-TEST-008
#[test]
fn test_cucumber_step_definitions_directory_exists() {
    let steps_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/steps");
    assert!(
        steps_dir.exists(),
        "cucumber step definitions directory must exist"
    );
}
