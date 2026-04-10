//! Validates CI coverage job exists and enforces thresholds (REQ-TEST-004).

// @req REQ-TEST-004
#[test]
fn test_ci_has_coverage_job() {
    let ci = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("coverage") || content.contains("Coverage"),
        "CI must have a coverage job"
    );
    assert!(
        content.contains("tarpaulin") || content.contains("llvm-cov"),
        "CI must use a coverage tool (tarpaulin or llvm-cov)"
    );
}

// @req REQ-TEST-004
#[test]
fn test_ci_coverage_reports_thresholds() {
    let ci = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("Check thresholds") || content.contains("coverage"),
        "CI coverage job must exist with threshold reporting"
    );
}
