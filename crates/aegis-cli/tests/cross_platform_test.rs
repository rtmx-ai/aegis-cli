//! Validates CI has a cross-platform test matrix covering Linux (RHEL via musl)
//! and Windows (MSVC) targets (REQ-TEST-011).

// rtmx:req REQ-TEST-011
#[test]
fn test_ci_has_cross_platform_matrix() {
    let ci = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("ubuntu-latest") || content.contains("ubuntu"),
        "CI must test on Linux"
    );
    assert!(
        content.contains("windows-latest") || content.contains("windows"),
        "CI must test on Windows"
    );
    // RHEL coverage: the musl static binary build and RPM smoke test
    // (rpm-package job with redhat/ubi9:latest container) cover RHEL.
    // Direct RHEL runners are not needed because the musl binary is
    // statically linked and verified in a UBI9 container.
    assert!(
        content.contains("x86_64-unknown-linux-musl"),
        "CI must build musl target for RHEL compatibility"
    );
}

// rtmx:req REQ-TEST-011
#[test]
fn test_ci_has_integration_tests_job() {
    let ci = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    assert!(
        content.contains("Integration Tests") || content.contains("integration"),
        "CI must have integration test job"
    );
}

// rtmx:req REQ-TEST-011
#[test]
fn test_ci_has_unit_tests_on_both_platforms() {
    let ci = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    // The matrix must include both OS targets in the unit test job
    assert!(
        content.contains("Unit Tests (${{ matrix.os }})"),
        "CI must have a matrix unit test job"
    );
    assert!(
        content.contains("os: [ubuntu-latest, windows-latest]"),
        "Unit test matrix must include both ubuntu and windows"
    );
}

// rtmx:req REQ-TEST-011
#[test]
fn test_ci_has_rhel_coverage_via_rpm_smoke_test() {
    let ci = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".github/workflows/ci.yml");
    let content = std::fs::read_to_string(&ci).unwrap();
    // RHEL is covered via the RPM package job that installs in a UBI9 container
    assert!(
        content.contains("redhat/ubi9") || content.contains("ubi9"),
        "CI must verify RPM installs on RHEL 9 (UBI9 container)"
    );
}
