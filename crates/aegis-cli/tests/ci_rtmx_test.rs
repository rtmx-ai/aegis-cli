//! Validates CI uses native rtmx CLI for test marker scanning (REQ-TEST-039).

// rtmx:req REQ-TEST-039
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
        "CI must run `rtmx from-tests` to scan rtmx:req markers"
    );
}
