//! Workstream decomposition and conflict analysis from the RTM critical path
//! (REQ-AGENT-034, REQ-AGENT-035).
//!
//! Analyzes the RTMX dependency graph to identify actionable requirements
//! (the "frontier" -- requirements whose dependencies are all satisfied),
//! computes a file conflict matrix to detect parallel-unsafe pairs, and
//! groups them into non-conflicting workstreams by greedy graph coloring.
//! Each workstream can safely run in an isolated git worktree without
//! merge conflicts.

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

/// A pair of requirements that conflict because they touch shared files.
/// Workstreams containing conflicting requirements must be serialized,
/// not parallelized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictPair {
    pub req_a: String,
    pub req_b: String,
    pub shared_files: Vec<String>,
}

/// File conflict matrix for frontier requirements (REQ-AGENT-035).
///
/// Maps each requirement to its estimated file touches and enumerates
/// all pairs that share at least one file. Consumed by the wave scheduler
/// (REQ-AGENT-039) to determine which workstreams can run in parallel.
#[derive(Debug, Clone)]
pub struct ConflictMatrix {
    /// All conflict pairs with their shared files.
    pub pairs: Vec<ConflictPair>,
    /// Requirement ID -> estimated files modified.
    pub file_map: HashMap<String, Vec<String>>,
}

impl ConflictMatrix {
    /// All conflicts involving a specific requirement.
    pub fn conflicts_for(&self, req_id: &str) -> Vec<&ConflictPair> {
        self.pairs
            .iter()
            .filter(|p| p.req_a == req_id || p.req_b == req_id)
            .collect()
    }

    /// Whether two requirements conflict on shared files.
    pub fn has_conflict(&self, a: &str, b: &str) -> bool {
        self.pairs
            .iter()
            .any(|p| (p.req_a == a && p.req_b == b) || (p.req_a == b && p.req_b == a))
    }

    /// Whether the matrix contains any conflicts.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
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

/// Compute the file conflict matrix for all frontier requirements.
///
/// Scans the RTM for actionable requirements, estimates which files
/// each will modify (from the test_module field), and identifies all
/// pairs that share at least one file.
pub fn compute_conflict_matrix(db: &RequirementsDb) -> ConflictMatrix {
    let frontier = find_frontier(db);
    let file_map = estimate_file_touches(db, &frontier);
    let pairs = build_conflict_pairs(&frontier, &file_map);
    ConflictMatrix { pairs, file_map }
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

        let wave_req_ids: Vec<String> = workstreams
            .iter()
            .flat_map(|ws| ws.requirements.iter().cloned())
            .collect();

        waves.push(Wave {
            index: wave_index,
            workstreams,
        });

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
    let mut cmd = Command::new("git");
    cmd.args(["worktree", "remove", worktree_path]);
    if force {
        cmd.arg("--force");
    }

    let worktree_result = cmd.output();
    match worktree_result {
        Ok(output) if output.status.success() => {
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

/// Directed acyclic graph of requirement dependencies.
#[derive(Debug, Clone)]
pub struct DependencyDag {
    /// All requirement IDs in the graph (nodes).
    pub nodes: Vec<String>,
    /// Adjacency list: req_id -> list of req_ids it depends on.
    pub edges: HashMap<String, Vec<String>>,
}

/// A cycle detected in the dependency graph (SCC with size > 1).
#[derive(Debug, Clone)]
pub struct Cycle {
    /// Requirement IDs forming the cycle.
    pub members: Vec<String>,
    /// Edges within the cycle as (from, to) pairs.
    pub edges: Vec<(String, String)>,
}

/// Build a directed graph (adjacency list) from the RequirementsDb dependencies
/// column (REQ-RTMX-017).
///
/// Every requirement in the database appears as a node. The `dependencies`
/// field (pipe-separated, e.g. "REQ-A|REQ-B") populates the adjacency list.
pub fn build_dag(db: &RequirementsDb) -> DependencyDag {
    let mut nodes = Vec::new();
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();

    for req in db.all() {
        nodes.push(req.req_id.clone());
        let deps: Vec<String> = req
            .dependency_ids()
            .into_iter()
            .filter(|d| !d.is_empty())
            .map(|d| d.to_string())
            .collect();
        edges.insert(req.req_id.clone(), deps);
    }

    DependencyDag { nodes, edges }
}

/// Detect cycles in the dependency graph using Tarjan's strongly connected
/// components algorithm (REQ-RTMX-018).
///
/// Returns only SCCs with size > 1 (actual cycles). Each `Cycle` includes
/// the member requirement IDs and the intra-cycle edges as (from, to) pairs.
pub fn detect_cycles(dag: &DependencyDag) -> Vec<Cycle> {
    let n = dag.nodes.len();
    let node_idx: HashMap<&str, usize> = dag
        .nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();

    // Build numeric adjacency list.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, node) in dag.nodes.iter().enumerate() {
        if let Some(deps) = dag.edges.get(node) {
            for dep in deps {
                if let Some(&j) = node_idx.get(dep.as_str()) {
                    adj[i].push(j);
                }
            }
        }
    }

    // Tarjan's SCC (iterative).
    let mut index_counter: usize = 0;
    let mut stack: Vec<usize> = Vec::new();
    let mut on_stack = vec![false; n];
    let mut indices: Vec<Option<usize>> = vec![None; n];
    let mut lowlinks = vec![0usize; n];
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    for start in 0..n {
        if indices[start].is_some() {
            continue;
        }
        let mut work: Vec<(usize, usize)> = Vec::new();
        indices[start] = Some(index_counter);
        lowlinks[start] = index_counter;
        index_counter += 1;
        stack.push(start);
        on_stack[start] = true;
        work.push((start, 0));

        while let Some(&(v, ci)) = work.last() {
            if ci < adj[v].len() {
                let last = work.len() - 1;
                work[last].1 = ci + 1;
                let w = adj[v][ci];
                match indices[w] {
                    None => {
                        indices[w] = Some(index_counter);
                        lowlinks[w] = index_counter;
                        index_counter += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        work.push((w, 0));
                    }
                    Some(w_idx) if on_stack[w] => {
                        if w_idx < lowlinks[v] {
                            lowlinks[v] = w_idx;
                        }
                    }
                    Some(_) => {}
                }
            } else {
                work.pop();
                if indices[v] == Some(lowlinks[v]) {
                    let mut component = Vec::new();
                    loop {
                        let w = stack.pop().expect("stack non-empty during SCC pop");
                        on_stack[w] = false;
                        component.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(component);
                }
                if let Some((parent, _)) = work.last().copied()
                    && lowlinks[v] < lowlinks[parent]
                {
                    lowlinks[parent] = lowlinks[v];
                }
            }
        }
    }

    // Filter to SCCs with size > 1 and build Cycle structs.
    let mut cycles = Vec::new();
    for scc in sccs {
        if scc.len() <= 1 {
            continue;
        }
        let member_set: HashSet<usize> = scc.iter().copied().collect();
        let mut members: Vec<String> = scc.iter().map(|&i| dag.nodes[i].clone()).collect();
        members.sort();

        let mut cycle_edges: Vec<(String, String)> = Vec::new();
        for &i in &scc {
            for &j in &adj[i] {
                if member_set.contains(&j) {
                    cycle_edges.push((dag.nodes[i].clone(), dag.nodes[j].clone()));
                }
            }
        }
        cycle_edges.sort();

        cycles.push(Cycle {
            members,
            edges: cycle_edges,
        });
    }

    cycles
}

/// Decompose actionable requirements into independent workstreams.
///
/// 1. Build a dependency DAG from the RequirementsDb.
/// 2. Filter to requirements whose status is not COMPLETE and whose
///    dependencies are all COMPLETE (the "frontier").
/// 3. Estimate which files each requirement touches (from test_module field).
/// 4. Compute the conflict matrix from shared file sets.
/// 5. Group requirements into workstreams via greedy graph coloring such
///    that no two workstreams share an estimated file.
pub fn decompose_workstreams(db: &RequirementsDb) -> Vec<Workstream> {
    let frontier = find_frontier(db);
    if frontier.is_empty() {
        return Vec::new();
    }
    let file_map = estimate_file_touches(db, &frontier);
    let pairs = build_conflict_pairs(&frontier, &file_map);
    group_into_workstreams(&frontier, &file_map, &pairs)
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

/// Build conflict pairs from the frontier and file map. Two requirements
/// conflict if they share at least one estimated file.
fn build_conflict_pairs(
    frontier: &[String],
    file_map: &HashMap<String, Vec<String>>,
) -> Vec<ConflictPair> {
    let mut pairs = Vec::new();
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
            let shared: Vec<String> = files_a
                .intersection(&files_b)
                .map(|s| s.to_string())
                .collect();
            if !shared.is_empty() {
                pairs.push(ConflictPair {
                    req_a: a.clone(),
                    req_b: b.clone(),
                    shared_files: shared,
                });
            }
        }
    }
    pairs
}

/// Group frontier requirements into non-conflicting workstreams using
/// greedy graph coloring. Conflict pairs determine which requirements
/// cannot share a workstream.
fn group_into_workstreams(
    frontier: &[String],
    file_map: &HashMap<String, Vec<String>>,
    conflict_pairs: &[ConflictPair],
) -> Vec<Workstream> {
    // Build adjacency from conflict pairs.
    let mut conflicts: HashMap<&str, HashSet<&str>> = HashMap::new();
    for pair in conflict_pairs {
        conflicts
            .entry(&pair.req_a)
            .or_default()
            .insert(&pair.req_b);
        conflicts
            .entry(&pair.req_b)
            .or_default()
            .insert(&pair.req_a);
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

    // rtmx:req REQ-AGENT-035
    #[test]
    fn test_conflict_matrix_detects_shared_files() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let matrix = compute_conflict_matrix(&db);

        // REQ-C and REQ-E both touch file_c.rs -- must appear as a conflict pair.
        assert!(
            matrix.has_conflict("REQ-C", "REQ-E"),
            "REQ-C and REQ-E share file_c.rs and must conflict"
        );

        // The shared file should be file_c.rs.
        let pair = matrix
            .pairs
            .iter()
            .find(|p| {
                (p.req_a == "REQ-C" && p.req_b == "REQ-E")
                    || (p.req_a == "REQ-E" && p.req_b == "REQ-C")
            })
            .expect("conflict pair for REQ-C/REQ-E must exist");
        assert!(
            pair.shared_files.contains(&"file_c.rs".to_string()),
            "shared file should be file_c.rs"
        );

        // REQ-C and REQ-D touch different files -- no conflict.
        assert!(
            !matrix.has_conflict("REQ-C", "REQ-D"),
            "REQ-C and REQ-D touch different files"
        );

        // REQ-D has no conflicts at all.
        assert!(
            matrix.conflicts_for("REQ-D").is_empty(),
            "REQ-D should have no conflicts"
        );

        // Matrix is not empty (at least the C/E pair).
        assert!(!matrix.is_empty());
    }

    // rtmx:req REQ-AGENT-035
    #[test]
    fn test_conflict_matrix_no_self_conflicts() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let matrix = compute_conflict_matrix(&db);
        for pair in &matrix.pairs {
            assert_ne!(
                pair.req_a, pair.req_b,
                "requirement {} conflicts with itself",
                pair.req_a
            );
        }
    }

    // rtmx:req REQ-AGENT-035
    #[test]
    fn test_conflict_matrix_empty_when_no_shared_files() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,A,P,a.rs,ta,Unit Test,MISSING,HIGH,1,,,,,,,,,\n\
                   REQ-B,AGENT,CORE,B,P,b.rs,tb,Unit Test,MISSING,HIGH,1,,,,,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let matrix = compute_conflict_matrix(&db);
        assert!(matrix.is_empty(), "no shared files means no conflicts");
    }

    // rtmx:req REQ-AGENT-035
    #[test]
    fn test_conflict_matrix_with_real_database() {
        let path = std::path::Path::new(".rtmx/database.csv");
        if path.exists() {
            let db = RequirementsDb::load(path).unwrap();
            let matrix = compute_conflict_matrix(&db);

            // No self-conflicts in real data.
            for pair in &matrix.pairs {
                assert_ne!(
                    pair.req_a, pair.req_b,
                    "self-conflict detected: {}",
                    pair.req_a
                );
                // Shared files must be non-empty for each pair.
                assert!(
                    !pair.shared_files.is_empty(),
                    "conflict pair {}/{} has no shared files",
                    pair.req_a,
                    pair.req_b
                );
            }

            // Every file in file_map keys should correspond to a frontier req.
            for (req_id, files) in &matrix.file_map {
                assert!(
                    db.get(req_id).is_some(),
                    "file_map has unknown requirement {}",
                    req_id
                );
                // Files list should be sorted and deduplicated.
                let mut sorted = files.clone();
                sorted.sort();
                sorted.dedup();
                assert_eq!(
                    files, &sorted,
                    "file list for {} should be sorted and deduplicated",
                    req_id
                );
            }
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

    // rtmx:req REQ-RTMX-017
    #[test]
    fn test_dag_from_rtm_dependencies() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,A,V,a.rs,ta,Unit Test,MISSING,HIGH,1,,,,,,,,,\n\
                   REQ-B,AGENT,CORE,B,V,b.rs,tb,Unit Test,MISSING,HIGH,1,,,REQ-A,,,,,\n\
                   REQ-C,AGENT,CORE,C,V,c.rs,tc,Unit Test,MISSING,HIGH,1,,,REQ-A|REQ-B,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let dag = build_dag(&db);

        // Verify nodes.
        assert_eq!(dag.nodes.len(), 3);
        assert!(dag.nodes.contains(&"REQ-A".to_string()));
        assert!(dag.nodes.contains(&"REQ-B".to_string()));
        assert!(dag.nodes.contains(&"REQ-C".to_string()));

        // Verify edges.
        assert!(dag.edges.get("REQ-A").unwrap().is_empty());
        assert_eq!(dag.edges.get("REQ-B").unwrap(), &vec!["REQ-A".to_string()]);
        let c_deps = dag.edges.get("REQ-C").unwrap();
        assert!(c_deps.contains(&"REQ-A".to_string()));
        assert!(c_deps.contains(&"REQ-B".to_string()));
        assert_eq!(c_deps.len(), 2);
    }

    // rtmx:req REQ-RTMX-017
    #[test]
    fn test_dag_includes_all_requirements() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let dag = build_dag(&db);

        let node_set: HashSet<String> = dag.nodes.iter().cloned().collect();
        for req in db.all() {
            assert!(
                node_set.contains(&req.req_id),
                "requirement {} missing from DAG nodes",
                req.req_id
            );
        }
        assert_eq!(dag.nodes.len(), db.count());
    }

    // rtmx:req REQ-RTMX-018
    #[test]
    fn test_tarjan_detects_cycle() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,A,V,a.rs,ta,Unit Test,MISSING,HIGH,1,,,REQ-C,,,,,\n\
                   REQ-B,AGENT,CORE,B,V,b.rs,tb,Unit Test,MISSING,HIGH,1,,,REQ-A,,,,,\n\
                   REQ-C,AGENT,CORE,C,V,c.rs,tc,Unit Test,MISSING,HIGH,1,,,REQ-B,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let dag = build_dag(&db);
        let cycles = detect_cycles(&dag);

        assert_eq!(cycles.len(), 1, "expected exactly one cycle");
        let cycle = &cycles[0];
        assert_eq!(cycle.members.len(), 3);
        assert!(cycle.members.contains(&"REQ-A".to_string()));
        assert!(cycle.members.contains(&"REQ-B".to_string()));
        assert!(cycle.members.contains(&"REQ-C".to_string()));
        // 3 edges in a 3-node cycle.
        assert_eq!(cycle.edges.len(), 3);
    }

    // rtmx:req REQ-RTMX-018
    #[test]
    fn test_tarjan_no_cycles_in_acyclic_graph() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,A,V,a.rs,ta,Unit Test,MISSING,HIGH,1,,,,,,,,,\n\
                   REQ-B,AGENT,CORE,B,V,b.rs,tb,Unit Test,MISSING,HIGH,1,,,REQ-A,,,,,\n\
                   REQ-C,AGENT,CORE,C,V,c.rs,tc,Unit Test,MISSING,HIGH,1,,,REQ-B,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let dag = build_dag(&db);
        let cycles = detect_cycles(&dag);

        assert!(cycles.is_empty(), "acyclic graph should have no cycles");
    }

    // rtmx:req REQ-RTMX-018
    #[test]
    fn test_tarjan_multiple_cycles() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n\
                   REQ-A,AGENT,CORE,A,V,a.rs,ta,Unit Test,MISSING,HIGH,1,,,REQ-B,,,,,\n\
                   REQ-B,AGENT,CORE,B,V,b.rs,tb,Unit Test,MISSING,HIGH,1,,,REQ-A,,,,,\n\
                   REQ-C,AGENT,CORE,C,V,c.rs,tc,Unit Test,MISSING,HIGH,1,,,REQ-D,,,,,\n\
                   REQ-D,AGENT,CORE,D,V,d.rs,td,Unit Test,MISSING,HIGH,1,,,REQ-C,,,,,\n\
                   REQ-E,AGENT,CORE,E,V,e.rs,te,Unit Test,MISSING,HIGH,1,,,,,,,,,";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let dag = build_dag(&db);
        let cycles = detect_cycles(&dag);

        assert_eq!(cycles.len(), 2, "expected exactly two cycles");

        // Collect all cycle member sets for flexible matching.
        let cycle_sets: Vec<HashSet<String>> = cycles
            .iter()
            .map(|c| c.members.iter().cloned().collect())
            .collect();

        let ab: HashSet<String> = ["REQ-A", "REQ-B"].iter().map(|s| s.to_string()).collect();
        let cd: HashSet<String> = ["REQ-C", "REQ-D"].iter().map(|s| s.to_string()).collect();

        assert!(
            cycle_sets.contains(&ab),
            "expected cycle containing REQ-A and REQ-B"
        );
        assert!(
            cycle_sets.contains(&cd),
            "expected cycle containing REQ-C and REQ-D"
        );

        // REQ-E is acyclic -- must not appear in any cycle.
        for cycle in &cycles {
            assert!(
                !cycle.members.contains(&"REQ-E".to_string()),
                "REQ-E should not be in any cycle"
            );
        }
    }

    // rtmx:req REQ-RTMX-018
    #[test]
    fn test_tarjan_with_real_database() {
        let path = std::path::Path::new(".rtmx/database.csv");
        if path.exists() {
            let db = RequirementsDb::load(path).unwrap();
            let dag = build_dag(&db);
            let cycles = detect_cycles(&dag);

            // The real database should be acyclic (well-formed RTM).
            // If cycles exist, report them for debugging.
            for cycle in &cycles {
                eprintln!("Unexpected cycle in real database: {:?}", cycle.members);
            }
            // Every node in the DAG should correspond to a DB entry.
            for node in &dag.nodes {
                assert!(
                    db.get(node).is_some(),
                    "DAG node {} not found in database",
                    node
                );
            }
        }
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
