//! REQ-RTMX-012: Cycle detection via Tarjan's strongly connected components.
//!
//! These tests exercise the `DependencyGraph::strongly_connected_components`
//! and `DependencyGraph::cycles` methods over small synthetic graphs built
//! from CSV fixtures via the public `from_db()` API.

use aegis_domain::rtmx::{DependencyGraph, RequirementsDb};

const CSV_HEADER: &str = "req_id,category,subcategory,requirement_text,target_value,test_module,\
test_function,validation_method,status,priority,phase,notes,dependencies\n";

/// Helper that normalises SCCs for comparison: sorts nodes within each SCC
/// and sorts the outer list by first element. Tarjan output ordering is
/// implementation-defined, so tests should be tolerant.
fn normalise(mut sccs: Vec<Vec<String>>) -> Vec<Vec<String>> {
    for scc in sccs.iter_mut() {
        scc.sort();
    }
    sccs.sort_by(|a, b| a.first().cmp(&b.first()));
    sccs
}

/// Build a `DependencyGraph` from a vector of (req_id, deps_csv) pairs.
fn graph_from_rows(rows: &[(&str, &str)]) -> DependencyGraph {
    let mut csv = String::from(CSV_HEADER);
    for (id, deps) in rows {
        csv.push_str(&format!("{id},CAT,X,x,t,,,,TODO,HIGH,1,,{deps}\n"));
    }
    let db = RequirementsDb::from_csv(&csv).unwrap();
    DependencyGraph::from_db(&db)
}

// rtmx:req REQ-RTMX-012
#[test]
fn test_cycle_detection_via_tarjan() {
    // A -> B -> C -> A (3-cycle)
    let g = graph_from_rows(&[("A", "B"), ("B", "C"), ("C", "A")]);

    let cycles = g.cycles();
    assert_eq!(cycles.len(), 1, "should detect exactly one cycle");
    let mut cycle = cycles[0].clone();
    cycle.sort();
    assert_eq!(
        cycle,
        vec!["A".to_string(), "B".to_string(), "C".to_string()]
    );
}

// rtmx:req REQ-RTMX-012
#[test]
fn test_no_cycles_in_dag() {
    // A -> B -> C (linear chain, no cycles)
    let g = graph_from_rows(&[("A", "B"), ("B", "C"), ("C", "")]);

    let cycles = g.cycles();
    assert!(cycles.is_empty(), "linear chain should have no cycles");

    // Tarjan's SCC should still report three singleton components
    // (since they are separate nodes with no back edges).
    let sccs = g.strongly_connected_components();
    assert_eq!(sccs.len(), 3, "each singleton is its own SCC");
}

// rtmx:req REQ-RTMX-012
#[test]
fn test_self_loop_is_cycle() {
    // A -> A (self-loop)
    let g = graph_from_rows(&[("A", "A")]);

    let cycles = g.cycles();
    assert_eq!(cycles.len(), 1, "self-loop should be detected as a cycle");
    assert_eq!(cycles[0], vec!["A".to_string()]);
}

// rtmx:req REQ-RTMX-012
#[test]
fn test_disjoint_cycles_both_detected() {
    // Cycle 1: A -> B -> A
    // Cycle 2: X -> Y -> Z -> X
    let g = graph_from_rows(&[("A", "B"), ("B", "A"), ("X", "Y"), ("Y", "Z"), ("Z", "X")]);

    let cycles = normalise(g.cycles());
    assert_eq!(cycles.len(), 2, "both disjoint cycles should surface");

    // First cycle: A, B
    assert_eq!(cycles[0], vec!["A".to_string(), "B".to_string()]);
    // Second cycle: X, Y, Z
    assert_eq!(
        cycles[1],
        vec!["X".to_string(), "Y".to_string(), "Z".to_string()]
    );
}

// rtmx:req REQ-RTMX-012
#[test]
fn test_singleton_without_self_loop_is_not_cycle() {
    // Isolated node with no edges.
    let g = graph_from_rows(&[("A", "")]);

    assert!(g.cycles().is_empty(), "isolated node is not a cycle");
    // But it IS an SCC (of size 1).
    let sccs = g.strongly_connected_components();
    assert_eq!(sccs.len(), 1);
    assert_eq!(sccs[0], vec!["A".to_string()]);
}
