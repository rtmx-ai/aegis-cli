//! Tests for system prompt tier templates.
//!
//! rtmx:req REQ-AGENT-050

/// Workspace root, resolved relative to the crate manifest directory.
fn workspace_root() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/aegis-agent -> workspace root is two levels up
    manifest_dir
        .parent()
        .expect("parent of crates/aegis-agent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn template_dir() -> std::path::PathBuf {
    workspace_root().join("templates").join("system_prompt")
}

fn read_template(name: &str) -> String {
    let path = template_dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

/// Rough token estimate: word_count * 1.3
fn estimate_tokens(text: &str) -> usize {
    let word_count = text.split_whitespace().count();
    (word_count as f64 * 1.3).ceil() as usize
}

// rtmx:req REQ-AGENT-050
#[test]
fn test_tier_templates_exist_with_markers() {
    let dir = template_dir();
    let expected_files = [
        "t0_identity.md",
        "t1_capabilities.md",
        "t2_categories.md.tmpl",
        "t3_requirements.md.tmpl",
        "system_prompt_header.md",
    ];

    for file in &expected_files {
        let path = dir.join(file);
        assert!(path.exists(), "template file missing: {}", path.display());
        let content = std::fs::read_to_string(&path).expect("readable");
        assert!(
            !content.trim().is_empty(),
            "template file is empty: {}",
            file
        );
    }
}

// rtmx:req REQ-AGENT-050
#[test]
fn test_t0_identity_within_token_budget() {
    let content = read_template("t0_identity.md");
    let tokens = estimate_tokens(&content);
    assert!(
        tokens <= 700,
        "T0 identity template exceeds 700 token budget: estimated {} tokens",
        tokens
    );
    // Sanity: must have meaningful content
    assert!(
        tokens >= 100,
        "T0 identity template suspiciously small: estimated {} tokens",
        tokens
    );
}

// rtmx:req REQ-AGENT-050
#[test]
fn test_t1_capabilities_within_token_budget() {
    let content = read_template("t1_capabilities.md");
    let tokens = estimate_tokens(&content);
    assert!(
        tokens <= 2000,
        "T1 capabilities template exceeds 2000 token budget: estimated {} tokens",
        tokens
    );
    assert!(
        tokens >= 200,
        "T1 capabilities template suspiciously small: estimated {} tokens",
        tokens
    );
}

// rtmx:req REQ-AGENT-050
#[test]
fn test_t2_template_has_category_slots() {
    let content = read_template("t2_categories.md.tmpl");
    assert!(
        content.contains("<!-- CATEGORIES_START -->"),
        "T2 template missing CATEGORIES_START marker"
    );
    assert!(
        content.contains("<!-- CATEGORIES_END -->"),
        "T2 template missing CATEGORIES_END marker"
    );
    assert!(
        content.contains("{{#each categories}}"),
        "T2 template missing Handlebars each-categories block"
    );
    assert!(
        content.contains("{{category_name}}"),
        "T2 template missing category_name slot"
    );
    assert!(
        content.contains("{{complete_count}}"),
        "T2 template missing complete_count slot"
    );
    assert!(
        content.contains("{{total_count}}"),
        "T2 template missing total_count slot"
    );
}

// rtmx:req REQ-AGENT-050
#[test]
fn test_t3_template_has_requirement_slots() {
    let content = read_template("t3_requirements.md.tmpl");
    assert!(
        content.contains("<!-- REQUIREMENTS_START -->"),
        "T3 template missing REQUIREMENTS_START marker"
    );
    assert!(
        content.contains("<!-- REQUIREMENTS_END -->"),
        "T3 template missing REQUIREMENTS_END marker"
    );
    assert!(
        content.contains("{{#each requirements}}"),
        "T3 template missing Handlebars each-requirements block"
    );
    assert!(
        content.contains("{{req_id}}"),
        "T3 template missing req_id slot"
    );
    assert!(
        content.contains("{{requirement_text}}"),
        "T3 template missing requirement_text slot"
    );
    assert!(
        content.contains("{{category}}"),
        "T3 template missing category slot"
    );
}

// rtmx:req REQ-AGENT-050
#[test]
fn test_tier_markers_are_valid() {
    let content = read_template("system_prompt_header.md");

    let required_markers = [
        "<!-- TIER:0 -->",
        "<!-- TIER:1 -->",
        "<!-- TIER:2 -->",
        "<!-- TIER:3 -->",
        "<!-- TIER:END -->",
    ];

    for marker in &required_markers {
        assert!(
            content.contains(marker),
            "header file missing tier marker: {}",
            marker
        );
    }

    // Verify markers appear in order
    let positions: Vec<usize> = required_markers
        .iter()
        .map(|m| {
            content
                .find(m)
                .unwrap_or_else(|| panic!("marker not found: {}", m))
        })
        .collect();

    for i in 1..positions.len() {
        assert!(
            positions[i] > positions[i - 1],
            "tier markers out of order: {} appears before {}",
            required_markers[i - 1],
            required_markers[i]
        );
    }
}
