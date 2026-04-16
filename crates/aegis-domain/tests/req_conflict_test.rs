//! REQ-RTMX-008: Requirement conflict detection.
//!
//! Exercises `DependencyGraph::detect_conflicts` which surfaces logical
//! contradictions in the requirement set: circular dependencies, dangling
//! references, and contradictory edges (A depends on B but B also claims to
//! block A).

use aegis_domain::rtmx::{DependencyGraph, RequirementConflict, RequirementsDb};

/// CSV header matching the full 19-column schema so `blocks` lives in the
/// correct positional column when parsed.
const CSV_HEADER: &str = "req_id,category,subcategory,requirement_text,target_value,test_module,\
test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,\
assignee,sprint,started_date,completed_date\n";

/// Build a CSV body from rows of (req_id, deps, blocks) where each row is
/// filled out with the full set of columns.
fn csv_from_rows(rows: &[(&str, &str, &str)]) -> String {
    let mut csv = String::from(CSV_HEADER);
    for (id, deps, blocks) in rows {
        // Columns after req_id:
        // category, subcategory, requirement_text, target_value, test_module,
        // test_function, validation_method, status, priority, phase, notes,
        // effort_weeks, dependencies, blocks, assignee, sprint, started_date,
        // completed_date
        csv.push_str(&format!(
            "{id},CAT,X,x,t,,,,MISSING,MEDIUM,1,,1,{deps},{blocks},,,,\n"
        ));
    }
    csv
}

fn build_graph(rows: &[(&str, &str, &str)]) -> (RequirementsDb, DependencyGraph) {
    let csv = csv_from_rows(rows);
    let db = RequirementsDb::from_csv(&csv).unwrap();
    let g = DependencyGraph::from_db(&db);
    (db, g)
}

// rtmx:req REQ-RTMX-008
#[test]
fn test_conflict_detection_finds_circular_dependencies() {
    // REQ-A depends on REQ-B, REQ-B depends on REQ-A -> cycle.
    let (db, g) = build_graph(&[("REQ-A", "REQ-B", ""), ("REQ-B", "REQ-A", "")]);

    let conflicts = g.detect_conflicts(&db);

    let cycles: Vec<&RequirementConflict> = conflicts
        .iter()
        .filter(|c| matches!(c, RequirementConflict::CircularDependency { .. }))
        .collect();

    assert_eq!(
        cycles.len(),
        1,
        "expected exactly one circular dep, got {conflicts:?}"
    );

    if let RequirementConflict::CircularDependency { members } = cycles[0] {
        let mut sorted = members.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["REQ-A".to_string(), "REQ-B".to_string()]);
    } else {
        panic!("expected CircularDependency variant");
    }
}

// rtmx:req REQ-RTMX-008
#[test]
fn test_dangling_dependency_detected() {
    // REQ-A depends on REQ-NONEXISTENT.
    let (db, g) = build_graph(&[("REQ-A", "REQ-NONEXISTENT", "")]);

    let conflicts = g.detect_conflicts(&db);

    let dangling: Vec<&RequirementConflict> = conflicts
        .iter()
        .filter(|c| matches!(c, RequirementConflict::DanglingDependency { .. }))
        .collect();

    assert_eq!(
        dangling.len(),
        1,
        "expected one dangling dependency, got {conflicts:?}"
    );

    if let RequirementConflict::DanglingDependency {
        req_id,
        missing_dep,
    } = dangling[0]
    {
        assert_eq!(req_id, "REQ-A");
        assert_eq!(missing_dep, "REQ-NONEXISTENT");
    } else {
        panic!("expected DanglingDependency variant");
    }
}

// rtmx:req REQ-RTMX-008
#[test]
fn test_dangling_blocks_detected() {
    // REQ-A declares blocks=REQ-NONEXISTENT.
    let (db, g) = build_graph(&[("REQ-A", "", "REQ-NONEXISTENT")]);

    let conflicts = g.detect_conflicts(&db);

    let dangling: Vec<&RequirementConflict> = conflicts
        .iter()
        .filter(|c| matches!(c, RequirementConflict::DanglingBlocks { .. }))
        .collect();

    assert_eq!(
        dangling.len(),
        1,
        "expected one dangling blocks entry, got {conflicts:?}"
    );

    if let RequirementConflict::DanglingBlocks {
        req_id,
        missing_target,
    } = dangling[0]
    {
        assert_eq!(req_id, "REQ-A");
        assert_eq!(missing_target, "REQ-NONEXISTENT");
    } else {
        panic!("expected DanglingBlocks variant");
    }
}

// rtmx:req REQ-RTMX-008
#[test]
fn test_contradictory_edge_detected() {
    // REQ-A depends on REQ-B, but REQ-B declares it blocks REQ-A.
    // Logical impossibility: A says "I need B" but B says "I block A".
    let (db, g) = build_graph(&[("REQ-A", "REQ-B", ""), ("REQ-B", "", "REQ-A")]);

    let conflicts = g.detect_conflicts(&db);

    let contradictions: Vec<&RequirementConflict> = conflicts
        .iter()
        .filter(|c| matches!(c, RequirementConflict::ContradictoryEdge { .. }))
        .collect();

    assert_eq!(
        contradictions.len(),
        1,
        "expected one contradictory edge, got {conflicts:?}"
    );

    if let RequirementConflict::ContradictoryEdge { req_a, req_b, .. } = contradictions[0] {
        // Exactly one edge between the same pair.
        assert!((req_a == "REQ-A" && req_b == "REQ-B") || (req_a == "REQ-B" && req_b == "REQ-A"));
    } else {
        panic!("expected ContradictoryEdge variant");
    }
}

// rtmx:req REQ-RTMX-008
#[test]
fn test_clean_db_returns_no_conflicts() {
    // REQ-A depends on REQ-B, REQ-B depends on REQ-C, no blocks, no cycles,
    // no dangling references.
    let (db, g) = build_graph(&[
        ("REQ-A", "REQ-B", ""),
        ("REQ-B", "REQ-C", ""),
        ("REQ-C", "", ""),
    ]);

    let conflicts = g.detect_conflicts(&db);
    assert!(
        conflicts.is_empty(),
        "clean db should have no conflicts, got {conflicts:?}"
    );
}

// rtmx:req REQ-RTMX-008
#[test]
fn test_consistent_blocks_entry_is_not_contradictory() {
    // REQ-A depends on REQ-B, REQ-B declares blocks=REQ-C (something else).
    // Not contradictory: A-B pair has no inverse edge.
    let (db, g) = build_graph(&[
        ("REQ-A", "REQ-B", ""),
        ("REQ-B", "", "REQ-C"),
        ("REQ-C", "", ""),
    ]);

    let conflicts = g.detect_conflicts(&db);
    let contradictions: Vec<&RequirementConflict> = conflicts
        .iter()
        .filter(|c| matches!(c, RequirementConflict::ContradictoryEdge { .. }))
        .collect();
    assert!(
        contradictions.is_empty(),
        "B blocking C while A depends on B is not contradictory, got {conflicts:?}"
    );
}

// rtmx:req REQ-RTMX-008
#[test]
fn test_real_database_conflicts() {
    // Informational: if the real RTM database exists, run detection and
    // print any conflicts found. Does not assert, but surfaces real RTM
    // bugs during test runs.
    let path = std::path::Path::new(".rtmx/database.csv");
    if !path.exists() {
        eprintln!("no .rtmx/database.csv present; skipping real-database check");
        return;
    }
    let db = match RequirementsDb::load(path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("could not load real database: {e}");
            return;
        }
    };
    let g = DependencyGraph::from_db(&db);
    let conflicts = g.detect_conflicts(&db);
    if conflicts.is_empty() {
        eprintln!("real RTM database: no conflicts detected");
    } else {
        eprintln!(
            "real RTM database: {} conflict(s) detected:",
            conflicts.len()
        );
        for c in &conflicts {
            eprintln!("  {c:?}");
        }
    }
}
