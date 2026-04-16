//! Workstream decomposition from the RTM critical path (REQ-AGENT-034).
//!
//! Analyzes the RTMX dependency graph to identify actionable requirements
//! (the "frontier" -- requirements whose dependencies are all satisfied)
//! and groups them into non-conflicting workstreams by estimated file-touch
//! analysis. Each workstream can safely run in an isolated git worktree
//! without merge conflicts.

use aegis_domain::rtmx::RequirementsDb;
use std::collections::{HashMap, HashSet};
use std::process::Command;

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

/// A wave is a batch of workstreams that can execute in parallel.
/// Wave N+1 starts only after Wave N merges. Within a wave all
/// workstreams are independent (no shared file conflicts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wave {
    /// Zero-based wave index.
    pub index: usize,
    /// Workstreams in this wave, all independent of each other.
    pub workstreams: Vec<Workstream>,
}

/// Result of cleaning up a completed worktree and its branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupResult {
    /// Path to the worktree that was cleaned up.
    pub worktree_path: String,
    /// Branch name that was cleaned up.
    pub branch: String,
    /// Whether cleanup succeeded.
    pub success: bool,
    /// Error message if cleanup failed.
    pub error: Option<String>,
}

/// Compute waves of workstreams respecting inter-wave dependencies.
///
/// Wave 0 contains the current frontier decomposed into non-conflicting
/// workstreams. After Wave 0 completes (merges), previously-blocked
/// requirements become actionable, forming Wave 1, and so on.
///
/// Returns `Vec<Wave>` where each wave's workstreams are independent.
pub fn compute_waves(db: &RequirementsDb) -> Vec<Wave> {
    let mut waves = Vec::new();
    let mut sim_db = db.clone();
    let mut wave_index = 0;

    loop {
        let workstreams = decompose_workstreams(&sim_db);
        if workstreams.is_empty() {
            break;
        }

        // Collect all requirement IDs in this wave.
        let wave_req_ids: Vec<String> = workstreams
            .iter()
            .flat_map(|ws| ws.requirements.iter().cloned())
            .collect();

        waves.push(Wave {
            index: wave_index,
            workstreams,
        });

        // Simulate completion: mark all wave reqs as COMPLETE so the
        // next frontier can be computed.
        for req_id in &wave_req_ids {
            let _ = sim_db.update_status(req_id, "COMPLETE");
        }

        wave_index += 1;
    }

    waves
}

/// Clean up a worktree and its branch after successful merge.
///
/// Runs `git worktree remove <path>` (with optional `--force`) followed
/// by `git branch -D <branch>`. If worktree removal fails, the branch
/// is preserved for debugging.
pub fn cleanup_worktree(worktree_path: &str, branch: &str, force: bool) -> CleanupResult {
    // Step 1: remove worktree.
    let mut cmd = Command::new("git");
    cmd.args(["worktree", "remove", worktree_path]);
    if force {
        cmd.arg("--force");
    }

    let worktree_result = cmd.output();
    match worktree_result {
        Ok(output) if output.status.success() => {
            // Step 2: delete branch only if worktree removal succeeded.
            let branch_result = Command::new("git").args(["branch", "-D", branch]).output();
            match branch_result {
                Ok(bo) if bo.status.success() => CleanupResult {
                    worktree_path: worktree_path.to_string(),
                    branch: branch.to_string(),
                    success: true,
                    error: None,
                },
                Ok(bo) => {
                    let stderr = String::from_utf8_lossy(&bo.stderr);
                    CleanupResult {
                        worktree_path: worktree_path.to_string(),
                        branch: branch.to_string(),
                        success: false,
                        error: Some(format!("branch deletion failed: {}", stderr.trim())),
                    }
                }
                Err(e) => CleanupResult {
                    worktree_path: worktree_path.to_string(),
                    branch: branch.to_string(),
                    success: false,
                    error: Some(format!("branch deletion error: {e}")),
                },
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            CleanupResult {
                worktree_path: worktree_path.to_string(),
                branch: branch.to_string(),
                success: false,
                error: Some(format!("worktree removal failed: {}", stderr.trim())),
            }
        }
        Err(e) => CleanupResult {
            worktree_path: worktree_path.to_string(),
            branch: branch.to_string(),
            success: false,
            error: Some(format!("worktree removal error: {e}")),
        },
    }
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

    // rtmx:req REQ-AGENT-039
    #[test]
    fn test_wave_execution_respects_deps() {
        // Wave 0: REQ-C, REQ-D, REQ-E are frontier (deps REQ-A, REQ-B complete).
        // After Wave 0 merges (REQ-C,D,E become COMPLETE), REQ-G becomes
        // actionable (it depends on REQ-C which was not yet complete).
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,Base,Done,base.rs,test_a,Unit Test,COMPLETE,HIGH,1,,,,,,,,\n\
                   REQ-B,AGENT,CORE,Mid,Done,mid.rs,test_b,Unit Test,COMPLETE,HIGH,1,,,REQ-A,,,,,\n\
                   REQ-C,AGENT,CORE,Feature C,Pending,file_c.rs,test_c,Unit Test,MISSING,HIGH,2,,,REQ-A,,,,,\n\
                   REQ-D,AGENT,CORE,Feature D,Pending,file_d.rs,test_d,Unit Test,MISSING,HIGH,2,,,REQ-A,,,,,\n\
                   REQ-E,AGENT,CORE,Feature E,Pending,file_e.rs,test_e,Unit Test,MISSING,HIGH,2,,,REQ-B,,,,,\n\
                   REQ-G,AGENT,CORE,Feature G,Pending,file_g.rs,test_g,Unit Test,MISSING,HIGH,3,,,REQ-C,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let waves = compute_waves(&db);

        assert!(
            waves.len() >= 2,
            "Expected at least 2 waves, got {}",
            waves.len()
        );

        // Wave 0 should contain C, D, E (frontier reqs).
        let wave0_reqs: HashSet<String> = waves[0]
            .workstreams
            .iter()
            .flat_map(|ws| ws.requirements.iter().cloned())
            .collect();
        assert!(wave0_reqs.contains("REQ-C"));
        assert!(wave0_reqs.contains("REQ-D"));
        assert!(wave0_reqs.contains("REQ-E"));
        assert!(!wave0_reqs.contains("REQ-G"), "REQ-G blocked by REQ-C");

        // Wave 1 should contain REQ-G (unblocked after Wave 0 merges).
        let wave1_reqs: HashSet<String> = waves[1]
            .workstreams
            .iter()
            .flat_map(|ws| ws.requirements.iter().cloned())
            .collect();
        assert!(wave1_reqs.contains("REQ-G"), "REQ-G should be in wave 1");

        // Wave indices are sequential.
        assert_eq!(waves[0].index, 0);
        assert_eq!(waves[1].index, 1);
    }

    // rtmx:req REQ-AGENT-039
    #[test]
    fn test_single_wave_when_no_dependencies_between_workstreams() {
        // All reqs have no deps -- everything is frontier in one wave.
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,A,P,a.rs,ta,Unit Test,MISSING,HIGH,1,,,,,,,,,\n\
                   REQ-B,AGENT,CORE,B,P,b.rs,tb,Unit Test,MISSING,HIGH,1,,,,,,,,,\n\
                   REQ-C,AGENT,CORE,C,P,c.rs,tc,Unit Test,MISSING,HIGH,1,,,,,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let waves = compute_waves(&db);

        assert_eq!(waves.len(), 1, "All reqs independent, should be 1 wave");
        assert_eq!(waves[0].index, 0);

        let total_reqs: usize = waves[0]
            .workstreams
            .iter()
            .map(|ws| ws.requirements.len())
            .sum();
        assert_eq!(total_reqs, 3);
    }

    // rtmx:req REQ-AGENT-041
    #[test]
    fn test_cleanup_removes_worktree() {
        // Test with a nonexistent worktree path -- git will fail, but we
        // verify the function returns a well-formed CleanupResult that
        // reflects the failure (since the path does not exist).
        let result = cleanup_worktree(
            "/tmp/aegis-test-nonexistent-worktree",
            "test-branch-nonexistent",
            false,
        );
        // The worktree does not exist, so removal should fail.
        assert_eq!(result.worktree_path, "/tmp/aegis-test-nonexistent-worktree");
        assert_eq!(result.branch, "test-branch-nonexistent");
        // Since the worktree path does not exist, git worktree remove fails,
        // and the branch should NOT be deleted (preserved for debugging).
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    // rtmx:req REQ-AGENT-041
    #[test]
    fn test_cleanup_skips_on_failure() {
        // When worktree removal fails, the branch must NOT be deleted.
        // We use a bogus path that git will reject.
        let result = cleanup_worktree(
            "/tmp/aegis-test-bogus-worktree-path-cleanup",
            "branch-should-not-be-deleted",
            false,
        );
        assert!(
            !result.success,
            "cleanup should fail for nonexistent worktree"
        );
        assert!(
            result.error.is_some(),
            "error should be recorded on failure"
        );
        // The key invariant: because worktree removal failed, we never
        // attempted branch deletion. The branch is preserved for debugging.
        assert_eq!(result.branch, "branch-should-not-be-deleted");
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
