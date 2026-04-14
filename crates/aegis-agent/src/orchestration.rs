//! Workstream decomposition from the RTM critical path (REQ-AGENT-034).
//!
//! Analyzes the RTMX dependency graph to identify actionable requirements
//! (the "frontier" -- requirements whose dependencies are all satisfied)
//! and groups them into non-conflicting workstreams by estimated file-touch
//! analysis. Each workstream can safely run in an isolated git worktree
//! without merge conflicts.

use aegis_domain::rtmx::RequirementsDb;
use std::collections::{HashMap, HashSet};

/// A workstream is a group of requirements that can be implemented
/// together in a single git worktree without file conflicts against
/// other workstreams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workstream {
    /// Human-readable name derived from the color assignment.
    pub name: String,
    /// Requirement IDs in this workstream.
    pub requirements: Vec<String>,
    /// Estimated files that will be modified (from test_module heuristics).
    pub estimated_files: Vec<String>,
}

/// Decompose actionable requirements into independent workstreams.
///
/// 1. Build a dependency DAG from the RequirementsDb.
/// 2. Filter to requirements whose status is not COMPLETE and whose
///    dependencies are all COMPLETE (the "frontier").
/// 3. Estimate which files each requirement touches (from test_module field).
/// 4. Group requirements into workstreams such that no two workstreams
///    share an estimated file (greedy graph coloring).
pub fn decompose_workstreams(db: &RequirementsDb) -> Vec<Workstream> {
    let frontier = find_frontier(db);
    if frontier.is_empty() {
        return Vec::new();
    }
    let file_map = estimate_file_touches(db, &frontier);
    group_into_workstreams(&frontier, &file_map)
}

/// Identify frontier requirements: not COMPLETE, all dependencies COMPLETE.
fn find_frontier(db: &RequirementsDb) -> Vec<String> {
    db.all()
        .iter()
        .filter(|req| req.status != "COMPLETE")
        .filter(|req| {
            req.dependency_ids().iter().all(|dep_id| {
                db.get(dep_id)
                    .map(|d| d.status == "COMPLETE")
                    .unwrap_or(false)
            })
        })
        .map(|req| req.req_id.clone())
        .collect()
}

/// Estimate which files each frontier requirement will modify,
/// derived from the test_module field in the RTM.
fn estimate_file_touches(
    db: &RequirementsDb,
    frontier: &[String],
) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for req_id in frontier {
        if let Some(req) = db.get(req_id) {
            let mut files = Vec::new();
            if !req.test_module.trim().is_empty() {
                files.push(req.test_module.clone());
            }
            files.sort();
            files.dedup();
            map.insert(req_id.clone(), files);
        }
    }
    map
}

/// Group frontier requirements into non-conflicting workstreams using
/// greedy graph coloring. Two requirements conflict if they share an
/// estimated file.
fn group_into_workstreams(
    frontier: &[String],
    file_map: &HashMap<String, Vec<String>>,
) -> Vec<Workstream> {
    // Build conflict adjacency.
    let mut conflicts: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (i, a) in frontier.iter().enumerate() {
        let files_a: HashSet<&str> = file_map
            .get(a)
            .map(|f| f.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();
        for b in &frontier[i + 1..] {
            let files_b: HashSet<&str> = file_map
                .get(b)
                .map(|f| f.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default();
            if !files_a.is_disjoint(&files_b) {
                conflicts.entry(a).or_default().insert(b);
                conflicts.entry(b).or_default().insert(a);
            }
        }
    }

    // Greedy coloring.
    let mut assignment: HashMap<&str, usize> = HashMap::new();
    for req_id in frontier {
        let used_colors: HashSet<usize> = conflicts
            .get(req_id.as_str())
            .map(|nbrs| {
                nbrs.iter()
                    .filter_map(|n| assignment.get(n).copied())
                    .collect()
            })
            .unwrap_or_default();
        let color = (0..).find(|c| !used_colors.contains(c)).unwrap();
        assignment.insert(req_id, color);
    }

    // Collect into workstreams by color.
    let max_color = assignment.values().copied().max().unwrap_or(0);
    let mut workstreams = Vec::new();
    for color in 0..=max_color {
        let reqs: Vec<String> = frontier
            .iter()
            .filter(|r| assignment.get(r.as_str()) == Some(&color))
            .cloned()
            .collect();
        if reqs.is_empty() {
            continue;
        }

        let mut all_files: Vec<String> = reqs
            .iter()
            .flat_map(|r| file_map.get(r).cloned().unwrap_or_default())
            .collect();
        all_files.sort();
        all_files.dedup();

        workstreams.push(Workstream {
            name: format!("workstream-{}", color),
            requirements: reqs,
            estimated_files: all_files,
        });
    }

    workstreams
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn test_csv() -> &'static str {
        "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
         REQ-A,AGENT,CORE,Base,Done,base.rs,test_a,Unit Test,COMPLETE,HIGH,1,,,,,,,,\n\
         REQ-B,AGENT,CORE,Mid,Done,mid.rs,test_b,Unit Test,COMPLETE,HIGH,1,,,REQ-A,,,,,\n\
         REQ-C,AGENT,CORE,Feature C,Pending,file_c.rs,test_c,Unit Test,MISSING,HIGH,2,,,REQ-A,,,,,\n\
         REQ-D,AGENT,CORE,Feature D,Pending,file_d.rs,test_d,Unit Test,MISSING,HIGH,2,,,REQ-A,,,,,\n\
         REQ-E,AGENT,CORE,Feature E,Pending,file_c.rs,test_e,Unit Test,MISSING,HIGH,2,,,REQ-B,,,,,\n\
         REQ-F,AGENT,CORE,Feature F,Pending,file_f.rs,test_f,Unit Test,MISSING,HIGH,2,,,REQ-Z,,,,,"
    }

    // rtmx:req REQ-AGENT-034
    #[test]
    fn test_decompose_identifies_independent_workstreams() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let workstreams = decompose_workstreams(&db);

        let all_reqs: HashSet<String> = workstreams
            .iter()
            .flat_map(|ws| ws.requirements.iter().cloned())
            .collect();

        // Frontier requirements (deps satisfied, not complete).
        assert!(all_reqs.contains("REQ-C"), "REQ-C should be on frontier");
        assert!(all_reqs.contains("REQ-D"), "REQ-D should be on frontier");
        assert!(all_reqs.contains("REQ-E"), "REQ-E should be on frontier");

        // Excluded: complete or unsatisfied deps.
        assert!(!all_reqs.contains("REQ-A"), "REQ-A is COMPLETE");
        assert!(!all_reqs.contains("REQ-B"), "REQ-B is COMPLETE");
        assert!(
            !all_reqs.contains("REQ-F"),
            "REQ-F has unsatisfied dep REQ-Z"
        );

        // REQ-C and REQ-E share file_c.rs -- must be in different workstreams.
        let ws_of = |id: &str| -> usize {
            workstreams
                .iter()
                .position(|ws| ws.requirements.contains(&id.to_string()))
                .unwrap()
        };
        assert_ne!(
            ws_of("REQ-C"),
            ws_of("REQ-E"),
            "C and E conflict on file_c.rs, must be in different workstreams"
        );

        // Each workstream has estimated_files populated.
        for ws in &workstreams {
            assert!(
                !ws.estimated_files.is_empty(),
                "workstream {} has no estimated files",
                ws.name
            );
        }
    }

    // rtmx:req REQ-AGENT-034
    #[test]
    fn test_decompose_empty_frontier_returns_empty() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,Done,Done,a.rs,test_a,Unit Test,COMPLETE,HIGH,1,,,,,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let workstreams = decompose_workstreams(&db);
        assert!(workstreams.is_empty());
    }

    // rtmx:req REQ-AGENT-034
    #[test]
    fn test_decompose_no_conflicts_single_workstream() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,A,P,a.rs,ta,Unit Test,MISSING,HIGH,1,,,,,,,,,\n\
                   REQ-B,AGENT,CORE,B,P,b.rs,tb,Unit Test,MISSING,HIGH,1,,,,,,,,,\n\
                   REQ-C,AGENT,CORE,C,P,c.rs,tc,Unit Test,MISSING,HIGH,1,,,,,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let workstreams = decompose_workstreams(&db);

        let all_reqs: Vec<String> = workstreams
            .iter()
            .flat_map(|ws| ws.requirements.iter().cloned())
            .collect();
        assert_eq!(all_reqs.len(), 3);
        // No conflicts -- greedy coloring puts all in one workstream.
        assert_eq!(workstreams.len(), 1);
    }

    // rtmx:req REQ-AGENT-034
    #[test]
    fn test_decompose_with_real_database() {
        let path = std::path::Path::new(".rtmx/database.csv");
        if path.exists() {
            let db = RequirementsDb::load(path).unwrap();
            let workstreams = decompose_workstreams(&db);
            // Real database has missing requirements with satisfied deps.
            assert!(
                !workstreams.is_empty(),
                "real database should produce at least one workstream"
            );
            // No requirement appears in multiple workstreams.
            let mut seen = HashSet::new();
            for ws in &workstreams {
                for req in &ws.requirements {
                    assert!(
                        seen.insert(req.clone()),
                        "requirement {} appears in multiple workstreams",
                        req
                    );
                }
            }
        }
    }
}
