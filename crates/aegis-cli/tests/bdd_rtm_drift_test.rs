//! Validates BDD-RTM drift detection tooling (REQ-TEST-040).

// rtmx:req REQ-TEST-040
#[test]
fn test_drift_script_exists() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/bdd-rtm-drift.py");
    assert!(script.exists(), "bdd-rtm-drift.py must exist");
}

// rtmx:req REQ-TEST-040
#[test]
fn test_drift_script_is_valid_python() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/bdd-rtm-drift.py");
    let content = std::fs::read_to_string(&script).unwrap();
    assert!(
        content.contains("database.csv"),
        "drift script must read the RTM database"
    );
    assert!(
        content.contains(".feature"),
        "drift script must scan feature files"
    );
}
