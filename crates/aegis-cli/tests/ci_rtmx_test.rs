//! Validates CI uses native rtmx CLI for test marker scanning (REQ-TEST-039).

// @req REQ-TEST-039
#[test]
fn test_ci_has_rtmx_from_tests_step() {
    let ci_yml = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci_yml).unwrap();
    assert!(
        content.contains("rtmx from-tests"),
        "CI must run `rtmx from-tests` to scan @req markers"
    );
}

// @req REQ-TEST-039
#[test]
fn test_ci_has_rtmx_verify_step() {
    let ci_yml = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci_yml).unwrap();
    assert!(
        content.contains("rtmx verify"),
        "CI must run `rtmx verify` for requirements traceability"
    );
}
