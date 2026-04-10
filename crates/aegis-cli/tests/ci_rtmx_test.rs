//! Validates CI runs rtmx-update-from-tests on every push (REQ-TEST-039).

// @req REQ-TEST-039
#[test]
fn test_ci_has_rtmx_update_step() {
    let ci_yml = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci_yml).unwrap();
    assert!(
        content.contains("rtmx-update-from-tests") || content.contains("from-tests"),
        "CI must run rtmx-update-from-tests or rtmx from-tests"
    );
}

// @req REQ-TEST-039
#[test]
fn test_rtmx_update_script_exists() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/rtmx-update-from-tests.py");
    assert!(script.exists(), "rtmx-update-from-tests.py must exist");
    let content = std::fs::read_to_string(&script).unwrap();
    assert!(
        content.contains("--dry-run"),
        "script must support --dry-run flag"
    );
}
