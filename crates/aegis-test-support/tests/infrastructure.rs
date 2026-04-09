//! Integration tests validating test infrastructure reliability.

use aegis_test_support::fixtures::{create_test_config, create_test_workspace, workspace_root};
use aegis_test_support::isolation::IsolatedHome;
use aegis_test_support::wiremock_llm::WireMockLlm;
use std::collections::HashSet;

// @req REQ-TEST-001
#[test]
fn test_no_shared_global_state() {
    // Two sequential IsolatedHome instances get completely independent paths.
    let path_a = {
        let home = IsolatedHome::new().expect("home A");
        home.path().to_path_buf()
    };
    let path_b = {
        let home = IsolatedHome::new().expect("home B");
        home.path().to_path_buf()
    };
    assert_ne!(path_a, path_b, "each test must get a unique HOME");
}

// @req REQ-TEST-006
#[test]
fn test_parallel_isolation() {
    // Filesystem writes in one IsolatedHome are invisible to the next.
    let sentinel = "sentinel.txt";
    {
        let home = IsolatedHome::new().expect("home A");
        std::fs::write(home.path().join(sentinel), b"leak").expect("write sentinel");
        assert!(home.path().join(sentinel).exists());
    }
    {
        let home = IsolatedHome::new().expect("home B");
        assert!(
            !home.path().join(sentinel).exists(),
            "sentinel from home A must not appear in home B"
        );
    }
}

// @req REQ-TEST-007
#[test]
fn test_fixture_factory_builds_valid_artifacts() {
    let (dir, workspace) = create_test_workspace();
    assert!(workspace.join("src/main.rs").exists(), "main.rs must exist");
    assert!(
        workspace.join(".aegisignore").exists(),
        ".aegisignore must exist"
    );

    let config_path = create_test_config(dir.path(), "local");
    assert!(config_path.exists(), "config.yaml must exist");
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        content.contains("mode: \"local\""),
        "config must contain mode"
    );
    assert!(
        content.contains("provider: local"),
        "config must contain provider"
    );
}

// @req REQ-TEST-014
#[test]
fn test_unique_tempdir_per_test() {
    let mut paths = HashSet::new();
    for _ in 0..5 {
        let home = IsolatedHome::new().expect("create home");
        let inserted = paths.insert(home.path().to_path_buf());
        assert!(inserted, "every IsolatedHome must have a unique path");
    }
    assert_eq!(paths.len(), 5);
}

// @req REQ-TEST-016
#[test]
fn test_order_independence() {
    // Running the same fixture setup twice yields structurally identical
    // but path-distinct results regardless of execution order.
    let (_, ws_a) = create_test_workspace();
    let (_, ws_b) = create_test_workspace();
    assert_ne!(ws_a, ws_b, "workspaces are path-distinct");
    // Both have the same structure.
    assert_eq!(
        ws_a.join("src/main.rs").exists(),
        ws_b.join("src/main.rs").exists()
    );
    assert_eq!(
        ws_a.join(".aegisignore").exists(),
        ws_b.join(".aegisignore").exists()
    );
}

// @req REQ-TEST-017
#[test]
fn test_tempdir_isolation() {
    let home_a = IsolatedHome::new().expect("home A");
    let file_a = home_a.aegis_dir().join("test_marker.json");
    std::fs::write(&file_a, b"{}").expect("write marker");
    let path_a = home_a.path().to_path_buf();
    drop(home_a);

    let home_b = IsolatedHome::new().expect("home B");
    // The marker from home_a must not exist in home_b.
    assert!(!home_b.aegis_dir().join("test_marker.json").exists());
    // And the old tempdir should be cleaned up.
    assert!(!path_a.exists(), "tempdir A should be removed after drop");
    drop(home_b);
}

// @req REQ-TEST-018
#[tokio::test]
async fn test_wiremock_ephemeral_port() {
    let mock_a = WireMockLlm::new().await;
    let mock_b = WireMockLlm::new().await;
    assert_ne!(
        mock_a.endpoint(),
        mock_b.endpoint(),
        "each WireMockLlm must bind to a unique ephemeral port"
    );
}

// @req REQ-TEST-019
#[test]
fn test_home_dir_mocked() {
    let real_home = std::env::var("HOME").ok();
    {
        let home = IsolatedHome::new().expect("create isolated home");
        let current = std::env::var("HOME").expect("HOME set");
        assert_eq!(
            std::path::PathBuf::from(&current),
            home.path().to_path_buf(),
            "HOME must point to the isolated tempdir"
        );
        if let Some(ref rh) = real_home {
            assert_ne!(
                &current, rh,
                "HOME must NOT point to the real home directory"
            );
        }
    }
    // After drop, HOME is restored.
    assert_eq!(std::env::var("HOME").ok(), real_home);
}

// @req REQ-BUILD-029
#[test]
fn test_watch_config_exists() {
    let root = workspace_root();
    let bacon_path = root.join("bacon.toml");
    assert!(
        bacon_path.exists(),
        "bacon.toml must exist at workspace root"
    );
    let content = std::fs::read_to_string(&bacon_path).unwrap();
    assert!(content.contains("[jobs.watch]"), "missing watch job");
    assert!(content.contains("[jobs.check]"), "missing check job");
    assert!(content.contains("[jobs.clippy]"), "missing clippy job");
    assert!(content.contains("[jobs.test]"), "missing test job");
    assert!(
        content.contains("kill_then_restart"),
        "watch must use kill_then_restart strategy"
    );
}
