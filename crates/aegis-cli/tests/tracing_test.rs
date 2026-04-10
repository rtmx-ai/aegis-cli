// @req REQ-BUILD-030
#[test]
fn test_tracing_subscriber_dependency_exists() {
    let cargo_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml).unwrap();
    assert!(
        content.contains("tracing-subscriber"),
        "aegis-cli must depend on tracing-subscriber"
    );
}

// @req REQ-BUILD-030
#[test]
fn test_tracing_dependency_in_workspace() {
    let root_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Cargo.toml");
    let content = std::fs::read_to_string(&root_toml).unwrap();
    assert!(
        content.contains("tracing =") || content.contains("tracing="),
        "workspace must have tracing in dependencies"
    );
}
