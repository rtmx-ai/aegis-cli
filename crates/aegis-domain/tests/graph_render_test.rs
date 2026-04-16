//! REQ-RTMX-013: Dependency graph visualization in DOT and Mermaid formats.
//!
//! These tests verify that `DependencyGraph::to_dot`, `to_mermaid`,
//! `to_dot_styled`, and `to_mermaid_styled` produce output with expected
//! edges, headers, and (optionally) status-based node colouring. Graphs
//! are built from CSV fixtures via the public `from_db()` API.

use aegis_domain::rtmx::{DependencyGraph, RequirementsDb};

const CHAIN_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-A,CAT,X,a,t,,,,TODO,HIGH,1,,REQ-B
REQ-B,CAT,X,b,t,,,,TODO,HIGH,1,,REQ-C
REQ-C,CAT,X,c,t,,,,TODO,HIGH,1,,";

const STATUS_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-A,CAT,X,a,t,,,,COMPLETE,CRITICAL,1,,REQ-B
REQ-B,CAT,X,b,t,,,,MISSING,CRITICAL,1,,REQ-C
REQ-C,CAT,X,c,t,,,,BLOCKED,CRITICAL,1,,";

// rtmx:req REQ-RTMX-013
#[test]
fn test_dot_format_renders_edges() {
    let db = RequirementsDb::from_csv(CHAIN_CSV).unwrap();
    let g = DependencyGraph::from_db(&db);

    let dot = g.to_dot();

    // Digraph header is present.
    assert!(dot.contains("digraph"), "output should be a digraph: {dot}");
    // Edges rendered with quoted node IDs.
    assert!(
        dot.contains("\"REQ-A\" -> \"REQ-B\""),
        "missing A -> B edge in: {dot}"
    );
    assert!(
        dot.contains("\"REQ-B\" -> \"REQ-C\""),
        "missing B -> C edge in: {dot}"
    );
    // Proper closing brace.
    assert!(dot.trim_end().ends_with('}'), "digraph must close: {dot}");
}

// rtmx:req REQ-RTMX-013
#[test]
fn test_mermaid_format_renders_edges() {
    let db = RequirementsDb::from_csv(CHAIN_CSV).unwrap();
    let g = DependencyGraph::from_db(&db);

    let mermaid = g.to_mermaid();

    // Mermaid flowchart header (top-down).
    assert!(
        mermaid.contains("graph TD"),
        "output should declare graph TD: {mermaid}"
    );
    // Edges use `-->` between bare node IDs.
    assert!(
        mermaid.contains("REQ-A --> REQ-B"),
        "missing A --> B in: {mermaid}"
    );
    assert!(
        mermaid.contains("REQ-B --> REQ-C"),
        "missing B --> C in: {mermaid}"
    );
}

// rtmx:req REQ-RTMX-013
#[test]
fn test_dot_styled_includes_status_colors() {
    let db = RequirementsDb::from_csv(STATUS_CSV).unwrap();
    let g = DependencyGraph::from_db(&db);

    let dot = g.to_dot_styled(&db);

    // COMPLETE node (A) should be green; MISSING (B) yellow; BLOCKED (C) red.
    assert!(
        dot.contains("fillcolor=\"green\"") || dot.contains("color=\"green\""),
        "COMPLETE node should be green: {dot}"
    );
    assert!(
        dot.contains("fillcolor=\"yellow\"") || dot.contains("color=\"yellow\""),
        "MISSING node should be yellow: {dot}"
    );
    assert!(
        dot.contains("fillcolor=\"red\"") || dot.contains("color=\"red\""),
        "BLOCKED node should be red: {dot}"
    );
    // Edges still present.
    assert!(dot.contains("\"REQ-A\" -> \"REQ-B\""));
}

// rtmx:req REQ-RTMX-013
#[test]
fn test_mermaid_styled_includes_status_classes() {
    let db = RequirementsDb::from_csv(STATUS_CSV).unwrap();
    let g = DependencyGraph::from_db(&db);

    let mermaid = g.to_mermaid_styled(&db);

    // Mermaid styled output assigns classes or styles per node.
    assert!(mermaid.contains("graph TD"));
    // A full colour legend must appear -- look for the three status colours
    // somewhere in the output (either via `style X fill:` or `classDef`).
    assert!(
        mermaid.contains("green") || mermaid.contains("#9f9") || mermaid.contains("lightgreen"),
        "COMPLETE node should have green styling: {mermaid}"
    );
    assert!(
        mermaid.contains("yellow") || mermaid.contains("#ff9") || mermaid.contains("#ffd"),
        "MISSING node should have yellow styling: {mermaid}"
    );
    assert!(
        mermaid.contains("red") || mermaid.contains("#f99") || mermaid.contains("salmon"),
        "BLOCKED node should have red styling: {mermaid}"
    );
}

// rtmx:req REQ-RTMX-013
#[test]
fn test_empty_graph_renders_valid_output() {
    // Minimal CSV with a single requirement and no dependencies -- this is
    // the smallest non-empty DB the parser accepts.
    let csv = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-SOLO,CAT,X,x,t,,,,TODO,HIGH,1,,";
    let db = RequirementsDb::from_csv(csv).unwrap();
    let g = DependencyGraph::from_db(&db);

    let dot = g.to_dot();
    assert!(
        dot.contains("digraph"),
        "empty DOT must still have header: {dot}"
    );
    assert!(
        dot.trim_end().ends_with('}'),
        "empty DOT must still close: {dot}"
    );

    let mermaid = g.to_mermaid();
    assert!(
        mermaid.contains("graph TD"),
        "empty Mermaid must still declare graph TD: {mermaid}"
    );
}
