//! Common test fixtures and helpers.

use tempfile::TempDir;
use std::path::PathBuf;

/// Create a temporary workspace directory with a basic project structure.
pub fn create_test_workspace() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let workspace = dir.path().to_path_buf();

    // Create a minimal project structure
    std::fs::create_dir_all(workspace.join("src")).expect("Failed to create src dir");
    std::fs::write(workspace.join("src/main.rs"), "fn main() {}\n").expect("Failed to write main.rs");
    std::fs::write(workspace.join(".aegisignore"), "*.pem\n.env\n").expect("Failed to write .aegisignore");

    (dir, workspace)
}

/// Create a temporary aegis config file for testing.
pub fn create_test_config(dir: &std::path::Path, mode: &str) -> PathBuf {
    let config_dir = dir.join(".aegis");
    std::fs::create_dir_all(&config_dir).expect("Failed to create .aegis dir");

    let config_path = config_dir.join("config.yaml");
    let config = format!(
        r#"version: "1.0"
mode: "{mode}"
backend:
  provider: local
  endpoint: "http://localhost:11434/v1"
"#
    );
    std::fs::write(&config_path, config).expect("Failed to write config");
    config_path
}
