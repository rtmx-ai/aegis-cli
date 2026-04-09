//! Validates the ubiquitous language glossary exists and covers core terms.

use aegis_test_support::fixtures::workspace_root;

// @req REQ-TEST-035
#[test]
fn test_glossary_exists_with_required_terms() {
    let glossary = workspace_root().join(".aegis/glossary.yaml");
    assert!(
        glossary.exists(),
        "glossary.yaml must exist at .aegis/glossary.yaml"
    );

    let content = std::fs::read_to_string(&glossary).unwrap();

    let required_terms = [
        "agent",
        "rea_loop",
        "hitl",
        "tool_call",
        "tool_risk",
        "session",
        "provider",
        "plugin",
        "cassette",
        "ledger",
        "aegisignore",
        "domain_event",
        "approval_gate",
    ];

    for term in &required_terms {
        assert!(
            content.contains(&format!("  {}:", term)),
            "glossary must define term: {}",
            term
        );
    }
}

// @req REQ-TEST-035
#[test]
fn test_glossary_terms_have_definitions_and_crates() {
    let glossary = workspace_root().join(".aegis/glossary.yaml");
    let content = std::fs::read_to_string(&glossary).unwrap();

    // Every term block must have both a definition and a crate reference.
    let term_count = content.matches("    definition:").count();
    let crate_count = content.matches("    crate:").count();
    assert!(
        term_count > 0,
        "glossary must have at least one term with a definition"
    );
    assert_eq!(
        term_count, crate_count,
        "every term must have both definition and crate fields"
    );
}
