//! Validates test pyramid balance metric script (REQ-TEST-038).

// rtmx:req REQ-TEST-038
#[test]
fn test_pyramid_balance_script_exists() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/test-pyramid-balance.py");
    assert!(script.exists(), "test-pyramid-balance.py must exist");
}

// rtmx:req REQ-TEST-038
#[test]
fn test_pyramid_balance_script_is_executable() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("scripts/test-pyramid-balance.py");
        let perms = std::fs::metadata(&script).unwrap().permissions();
        assert!(
            perms.mode() & 0o111 != 0,
            "test-pyramid-balance.py must be executable"
        );
    }
}

// rtmx:req REQ-TEST-038
#[test]
fn test_pyramid_balance_script_runs_successfully() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/test-pyramid-balance.py");
    let output = std::process::Command::new("python3")
        .arg(&script)
        .output()
        .expect("python3 must be available to run test-pyramid-balance.py");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Test Pyramid Balance Report"),
        "Script should produce a balance report, got: {stdout}"
    );
}
