//! REQ-RTMX-007: Priority and critical-path analysis.
//!
//! Exercises `DependencyGraph::transitive_blocks`, `priority_scores`, and
//! `critical_path` which rank the actionable frontier of MISSING
//! requirements by downstream leverage (priority weight times transitive
//! dependents).

use aegis_domain::rtmx::{DependencyGraph, PriorityScore, RequirementsDb};

/// Full 19-column header.
const CSV_HEADER: &str = "req_id,category,subcategory,requirement_text,target_value,test_module,\
test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,\
assignee,sprint,started_date,completed_date\n";

/// Build a CSV body. `rows` entries are (id, status, priority, effort_weeks, deps).
fn csv_from_rows(rows: &[(&str, &str, &str, &str, &str)]) -> String {
    let mut csv = String::from(CSV_HEADER);
    for (id, status, priority, effort, deps) in rows {
        csv.push_str(&format!(
            "{id},CAT,X,x,t,,,,{status},{priority},1,,{effort},{deps},,,,,\n"
        ));
    }
    csv
}

fn build(rows: &[(&str, &str, &str, &str, &str)]) -> (RequirementsDb, DependencyGraph) {
    let csv = csv_from_rows(rows);
    let db = RequirementsDb::from_csv(&csv).unwrap();
    let g = DependencyGraph::from_db(&db);
    (db, g)
}

fn find<'a>(scores: &'a [PriorityScore], id: &str) -> &'a PriorityScore {
    scores
        .iter()
        .find(|s| s.req_id == id)
        .unwrap_or_else(|| panic!("expected score for {id} in {scores:?}"))
}

// rtmx:req REQ-RTMX-007
#[test]
fn test_critical_path_finds_highest_blocking_unblocked_req() {
    // Chain A -> B -> C -> D where each row's dependencies column points
    // to the previous letter. A is already COMPLETE. B, C, D are MISSING.
    // After A completes, only B has satisfied dependencies (A is COMPLETE).
    // B transitively blocks C and D; C transitively blocks D; D blocks
    // nothing. So the critical path should rank B first.
    let (db, g) = build(&[
        ("REQ-A", "COMPLETE", "HIGH", "1", ""),
        ("REQ-B", "MISSING", "HIGH", "1", "REQ-A"),
        ("REQ-C", "MISSING", "HIGH", "1", "REQ-B"),
        ("REQ-D", "MISSING", "HIGH", "1", "REQ-C"),
    ]);

    let path = g.critical_path(&db);
    assert!(!path.is_empty(), "critical path should not be empty");

    // B is the only actionable requirement (C and D have unsatisfied deps).
    // So the critical path should start with B.
    assert_eq!(path[0].req_id, "REQ-B");
    assert_eq!(
        path[0].transitive_blocks, 2,
        "B transitively blocks C and D"
    );
}

// rtmx:req REQ-RTMX-007
#[test]
fn test_priority_score_combines_priority_and_blocks() {
    // High-priority req with 3 transitive blocks should outrank a medium-
    // priority req with 1 block.
    //
    // Graph layout (all MISSING, no dependencies so all are actionable):
    //   A (HIGH) <- B, C, D   => A is depended on by B, C, D
    //   X (MEDIUM) <- Y        => X is depended on by Y
    // Transitive blocks for A = 3 (B, C, D).
    // Transitive blocks for X = 1 (Y).
    // A_score = 3.0 * (1 + 3) = 12.0
    // X_score = 2.0 * (1 + 1) = 4.0
    let (db, g) = build(&[
        ("REQ-A", "MISSING", "HIGH", "1", ""),
        ("REQ-B", "MISSING", "MEDIUM", "1", "REQ-A"),
        ("REQ-C", "MISSING", "MEDIUM", "1", "REQ-A"),
        ("REQ-D", "MISSING", "MEDIUM", "1", "REQ-A"),
        ("REQ-X", "MISSING", "MEDIUM", "1", ""),
        ("REQ-Y", "MISSING", "MEDIUM", "1", "REQ-X"),
    ]);

    let scores = g.priority_scores(&db);
    let a = find(&scores, "REQ-A");
    let x = find(&scores, "REQ-X");
    assert_eq!(a.direct_blocks, 3);
    assert_eq!(a.transitive_blocks, 3);
    assert!(
        a.score > x.score,
        "HIGH with 3 blocks ({}) should outrank MEDIUM with 1 block ({})",
        a.score,
        x.score
    );

    // Critical path orders descending by score, so A must appear before X.
    let path = g.critical_path(&db);
    let pos = |id: &str| path.iter().position(|s| s.req_id == id).unwrap();
    assert!(pos("REQ-A") < pos("REQ-X"));
}

// rtmx:req REQ-RTMX-007
#[test]
fn test_critical_path_excludes_completed_requirements() {
    // A is COMPLETE; it must not appear in the critical path.
    let (db, g) = build(&[
        ("REQ-A", "COMPLETE", "HIGH", "1", ""),
        ("REQ-B", "MISSING", "HIGH", "1", "REQ-A"),
    ]);

    let path = g.critical_path(&db);
    assert!(
        path.iter().all(|s| s.req_id != "REQ-A"),
        "COMPLETE requirement must not appear in critical path"
    );
    assert!(path.iter().any(|s| s.req_id == "REQ-B"));
}

// rtmx:req REQ-RTMX-007
#[test]
fn test_critical_path_excludes_blocked_requirements() {
    // B depends on A (MISSING). B's dependencies are NOT satisfied, so B
    // must not appear in the actionable critical path.
    let (db, g) = build(&[
        ("REQ-A", "MISSING", "HIGH", "1", ""),
        ("REQ-B", "MISSING", "HIGH", "1", "REQ-A"),
    ]);

    let path = g.critical_path(&db);
    let ids: Vec<&str> = path.iter().map(|s| s.req_id.as_str()).collect();
    assert!(ids.contains(&"REQ-A"), "A is actionable, should be present");
    assert!(
        !ids.contains(&"REQ-B"),
        "B has unsatisfied dependency A, must not be in critical path"
    );
}

// rtmx:req REQ-RTMX-007
#[test]
fn test_transitive_blocks_counts_downstream() {
    // A <- B <- C (A is depended on by B, B by C). Transitively, A blocks
    // B and C. B blocks C.
    let (_db, g) = build(&[
        ("REQ-A", "MISSING", "HIGH", "1", ""),
        ("REQ-B", "MISSING", "HIGH", "1", "REQ-A"),
        ("REQ-C", "MISSING", "HIGH", "1", "REQ-B"),
    ]);

    assert_eq!(g.transitive_blocks("REQ-A"), 2);
    assert_eq!(g.transitive_blocks("REQ-B"), 1);
    assert_eq!(g.transitive_blocks("REQ-C"), 0);
}
