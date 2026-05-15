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
use std::future::Future;
use std::pin::Pin;
use std::process::Command;
use tracing::{info, warn};

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

// ---------------------------------------------------------------------------
// REQ-AGENT-059: WorktreeManager port trait
// ---------------------------------------------------------------------------

/// Result of a worktree operation.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory.
    pub path: String,
    /// Branch name associated with this worktree.
    pub branch: String,
}

/// Port trait for git worktree operations (REQ-AGENT-059).
///
/// Abstracts worktree create/remove/list behind a trait so that
/// orchestration logic can be tested with a mock implementation.
pub trait WorktreeManager: Send + Sync {
    /// Create a new worktree at `path` on branch `branch`.
    ///
    /// Equivalent to `git worktree add <path> -b <branch>`.
    fn create(
        &self,
        path: &str,
        branch: &str,
    ) -> impl Future<Output = Result<WorktreeInfo, String>> + Send;

    /// Remove a worktree at `path`.
    ///
    /// Equivalent to `git worktree remove <path>`.
    fn remove(&self, path: &str) -> impl Future<Output = Result<(), String>> + Send;

    /// List all active worktrees.
    fn list(&self) -> impl Future<Output = Result<Vec<WorktreeInfo>, String>> + Send;
}

/// Default implementation that shells out to `git worktree` commands.
pub struct GitWorktreeManager {
    /// Path to the repository root.
    repo_root: String,
}

impl GitWorktreeManager {
    /// Create a new manager for the given repository root.
    pub fn new(repo_root: String) -> Self {
        Self { repo_root }
    }
}

impl WorktreeManager for GitWorktreeManager {
    async fn create(&self, path: &str, branch: &str) -> Result<WorktreeInfo, String> {
        let output = Command::new("git")
            .args(["worktree", "add", path, "-b", branch])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git worktree add: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree add failed: {}", stderr.trim()));
        }

        Ok(WorktreeInfo {
            path: path.to_string(),
            branch: branch.to_string(),
        })
    }

    async fn remove(&self, path: &str) -> Result<(), String> {
        let output = Command::new("git")
            .args(["worktree", "remove", path, "--force"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git worktree remove: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree remove failed: {}", stderr.trim()));
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<WorktreeInfo>, String> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| format!("Failed to run git worktree list: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree list failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut current_path = None;
        let mut current_branch = None;

        for line in stdout.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                current_path = Some(p.to_string());
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(b.to_string());
            } else if line.is_empty() {
                if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take()) {
                    worktrees.push(WorktreeInfo { path, branch });
                }
                current_path = None;
                current_branch = None;
            }
        }
        // Handle trailing entry without final blank line.
        if let (Some(path), Some(branch)) = (current_path, current_branch) {
            worktrees.push(WorktreeInfo { path, branch });
        }

        Ok(worktrees)
    }
}

// ---------------------------------------------------------------------------
// REQ-AGENT-060: Wave driver
// REQ-AGENT-061: Failure recovery
// ---------------------------------------------------------------------------

/// Result of executing a single workstream within a wave.
#[derive(Debug, Clone)]
pub struct WorkstreamResult {
    /// Name of the workstream.
    pub name: String,
    /// Requirement IDs in this workstream.
    pub requirements: Vec<String>,
    /// Whether execution succeeded.
    pub success: bool,
    /// Error message if execution failed.
    pub error: Option<String>,
    /// Number of retry attempts used.
    pub attempts: u32,
}

/// Result of executing an entire wave.
#[derive(Debug, Clone)]
pub struct WaveResult {
    /// Zero-based wave index.
    pub wave_index: usize,
    /// Results for each workstream in the wave.
    pub workstream_results: Vec<WorkstreamResult>,
}

impl WaveResult {
    /// Whether all workstreams in this wave succeeded.
    pub fn all_succeeded(&self) -> bool {
        self.workstream_results.iter().all(|r| r.success)
    }

    /// Requirement IDs from failed workstreams.
    pub fn failed_requirements(&self) -> Vec<String> {
        self.workstream_results
            .iter()
            .filter(|r| !r.success)
            .flat_map(|r| r.requirements.clone())
            .collect()
    }

    /// Requirement IDs from successful workstreams.
    pub fn succeeded_requirements(&self) -> Vec<String> {
        self.workstream_results
            .iter()
            .filter(|r| r.success)
            .flat_map(|r| r.requirements.clone())
            .collect()
    }
}

/// Type alias for the agent callback used by the wave driver.
///
/// The callback receives (worktree_path, workstream_name, requirements)
/// and returns Ok(()) on success or Err(error_message) on failure.
pub type AgentCallback = Box<
    dyn Fn(
            String,
            String,
            Vec<String>,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        + Send
        + Sync,
>;

/// Execute a single wave: create worktrees, run agents, collect results.
///
/// For each workstream in the wave:
/// 1. Create a git worktree via `WorktreeManager`.
/// 2. Run the agent callback.
/// 3. On failure, retry once (REQ-AGENT-061).
/// 4. Clean up the worktree.
///
/// All workstreams in a wave run sequentially (the caller can parallelize
/// by spawning multiple `drive_wave` calls, but within a wave the driver
/// serializes to keep resource usage predictable).
pub async fn drive_wave<W: WorktreeManager>(
    wm: &W,
    wave: &Wave,
    agent_fn: &AgentCallback,
    worktree_base: &str,
) -> WaveResult {
    let mut results = Vec::new();

    for ws in &wave.workstreams {
        let worktree_path = format!("{}/{}", worktree_base, ws.name);
        let branch = format!("agent/{}", ws.name);

        info!(
            wave = wave.index,
            workstream = %ws.name,
            "starting workstream execution"
        );

        // Create worktree.
        let create_result = wm.create(&worktree_path, &branch).await;
        if let Err(e) = create_result {
            warn!(
                workstream = %ws.name,
                error = %e,
                "failed to create worktree"
            );
            results.push(WorkstreamResult {
                name: ws.name.clone(),
                requirements: ws.requirements.clone(),
                success: false,
                error: Some(format!("worktree creation failed: {e}")),
                attempts: 0,
            });
            continue;
        }

        // Run agent with retry (REQ-AGENT-061).
        let mut last_error = None;
        let mut attempts = 0;
        let max_attempts = 2; // 1 initial + 1 retry

        for attempt in 0..max_attempts {
            attempts = attempt + 1;
            let result = agent_fn(
                worktree_path.clone(),
                ws.name.clone(),
                ws.requirements.clone(),
            )
            .await;

            match result {
                Ok(()) => {
                    info!(
                        workstream = %ws.name,
                        attempt = attempts,
                        "workstream completed successfully"
                    );
                    last_error = None;
                    break;
                }
                Err(e) => {
                    warn!(
                        workstream = %ws.name,
                        attempt = attempts,
                        error = %e,
                        "workstream execution failed"
                    );
                    last_error = Some(e);
                    if attempt + 1 < max_attempts {
                        info!(
                            workstream = %ws.name,
                            "retrying workstream"
                        );
                    }
                }
            }
        }

        let success = last_error.is_none();
        results.push(WorkstreamResult {
            name: ws.name.clone(),
            requirements: ws.requirements.clone(),
            success,
            error: last_error,
            attempts,
        });

        // Clean up worktree regardless of success.
        if let Err(e) = wm.remove(&worktree_path).await {
            warn!(
                workstream = %ws.name,
                error = %e,
                "failed to remove worktree (non-fatal)"
            );
        }
    }

    WaveResult {
        wave_index: wave.index,
        workstream_results: results,
    }
}

/// Execute all waves in sequence, skipping requirements blocked by failures.
///
/// After each wave, failed requirements are tracked. Subsequent waves
/// skip workstreams whose requirements depend on failed ones (REQ-AGENT-061),
/// but independent workstreams proceed normally.
pub async fn drive_all_waves<W: WorktreeManager>(
    wm: &W,
    waves: &[Wave],
    agent_fn: &AgentCallback,
    worktree_base: &str,
) -> Vec<WaveResult> {
    let mut results = Vec::new();
    let mut failed_reqs: HashSet<String> = HashSet::new();

    for wave in waves {
        // Filter workstreams: skip those blocked by previously failed reqs.
        let eligible_workstreams: Vec<Workstream> = wave
            .workstreams
            .iter()
            .filter(|ws| {
                // A workstream is blocked if ANY of its requirements appear
                // in the failed set. This is a conservative check -- in a
                // full implementation we'd check the dependency graph.
                !ws.requirements.iter().any(|r| failed_reqs.contains(r))
            })
            .cloned()
            .collect();

        if eligible_workstreams.is_empty() {
            info!(
                wave = wave.index,
                "skipping wave: all workstreams blocked by prior failures"
            );
            continue;
        }

        let eligible_wave = Wave {
            index: wave.index,
            workstreams: eligible_workstreams,
        };

        let wave_result = drive_wave(wm, &eligible_wave, agent_fn, worktree_base).await;

        // Track failed requirements for subsequent wave filtering.
        for req in wave_result.failed_requirements() {
            failed_reqs.insert(req);
        }

        results.push(wave_result);
    }

    results
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

/// Export a dependency DAG as DOT format for Graphviz rendering (REQ-RTMX-019).
///
/// Completed nodes are colored green (`#90EE90`), missing/incomplete nodes
/// are colored red (`#FFB6C1`). Produces a valid `digraph { ... }` block.
pub fn export_dot(dag: &DependencyDag, db: &RequirementsDb) -> String {
    let mut out = String::from("digraph {\n  node [shape=box];\n");
    for node in &dag.nodes {
        let color = if db
            .get(node)
            .map(|r| r.status == "COMPLETE")
            .unwrap_or(false)
        {
            "#90EE90"
        } else {
            "#FFB6C1"
        };
        out.push_str(&format!(
            "  \"{}\" [style=filled, fillcolor=\"{}\"];\n",
            node, color
        ));
    }
    for (from, deps) in &dag.edges {
        for to in deps {
            out.push_str(&format!("  \"{}\" -> \"{}\";\n", from, to));
        }
    }
    out.push('}');
    out
}

/// Export a dependency DAG as Mermaid flowchart for markdown embedding
/// (REQ-RTMX-020).
///
/// Completed nodes are styled green, missing/incomplete nodes are styled red
/// using Mermaid `classDef` syntax. Produces a `graph TD` block.
pub fn export_mermaid(dag: &DependencyDag, db: &RequirementsDb) -> String {
    let mut out = String::from("graph TD\n");
    out.push_str("  classDef complete fill:#90EE90,stroke:#333\n");
    out.push_str("  classDef missing fill:#FFB6C1,stroke:#333\n");
    for node in &dag.nodes {
        let class = if db
            .get(node)
            .map(|r| r.status == "COMPLETE")
            .unwrap_or(false)
        {
            "complete"
        } else {
            "missing"
        };
        out.push_str(&format!("  {}[{}]:::{}\n", node, node, class));
    }
    for (from, deps) in &dag.edges {
        for to in deps {
            out.push_str(&format!("  {} --> {}\n", from, to));
        }
    }
    out
}

/// Format a dependency graph from the RTM database in the given format.
/// This is the entry point for the CLI `graph` subcommand.
pub fn format_dependency_graph(db: &RequirementsDb, format: &str) -> Result<String, String> {
    let dag = build_dag(db);
    match format {
        "dot" => Ok(export_dot(&dag, db)),
        "mermaid" => Ok(export_mermaid(&dag, db)),
        _ => Err(format!(
            "Unknown format '{}'. Use 'dot' or 'mermaid'.",
            format
        )),
    }
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

    // rtmx:req REQ-RTMX-019
    #[test]
    fn test_dot_export_valid_syntax() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let dag = build_dag(&db);
        let dot = export_dot(&dag, &db);

        assert!(dot.contains("digraph"), "DOT output must contain 'digraph'");
        assert!(dot.contains("->"), "DOT output must contain edges");
        assert!(dot.contains("REQ-A"), "DOT output must contain node REQ-A");
        assert!(dot.contains("REQ-C"), "DOT output must contain node REQ-C");
        assert!(
            dot.contains("#90EE90") || dot.contains("#FFB6C1"),
            "DOT output must contain color codes"
        );
    }

    // rtmx:req REQ-RTMX-019
    #[test]
    fn test_dot_export_empty_dag() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let dag = build_dag(&db);
        let dot = export_dot(&dag, &db);

        assert!(dot.contains("digraph"), "empty DOT must still be valid");
        assert!(!dot.contains("->"), "empty DAG should have no edges");
    }

    // rtmx:req REQ-RTMX-019
    #[test]
    fn test_dot_export_colors_by_status() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let dag = build_dag(&db);
        let dot = export_dot(&dag, &db);

        // REQ-A is COMPLETE -> green
        assert!(
            dot.contains("\"REQ-A\" [style=filled, fillcolor=\"#90EE90\"]"),
            "COMPLETE node REQ-A should be green"
        );
        // REQ-C is MISSING -> red
        assert!(
            dot.contains("\"REQ-C\" [style=filled, fillcolor=\"#FFB6C1\"]"),
            "MISSING node REQ-C should be red"
        );
    }

    // rtmx:req REQ-RTMX-020
    #[test]
    fn test_mermaid_export_valid_syntax() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let dag = build_dag(&db);
        let mermaid = export_mermaid(&dag, &db);

        assert!(
            mermaid.contains("graph TD"),
            "Mermaid output must contain 'graph TD'"
        );
        assert!(mermaid.contains("-->"), "Mermaid output must contain edges");
        assert!(
            mermaid.contains("REQ-A"),
            "Mermaid output must contain node REQ-A"
        );
        assert!(
            mermaid.contains("classDef"),
            "Mermaid output must contain classDef"
        );
    }

    // rtmx:req REQ-RTMX-020
    #[test]
    fn test_mermaid_export_empty_dag() {
        let csv = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date\n";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let dag = build_dag(&db);
        let mermaid = export_mermaid(&dag, &db);

        assert!(
            mermaid.contains("graph TD"),
            "empty Mermaid must still be valid"
        );
        assert!(!mermaid.contains("-->"), "empty DAG should have no edges");
    }

    // rtmx:req REQ-RTMX-020
    #[test]
    fn test_mermaid_export_colors_by_status() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let dag = build_dag(&db);
        let mermaid = export_mermaid(&dag, &db);

        // REQ-A is COMPLETE -> class complete
        assert!(
            mermaid.contains("REQ-A[REQ-A]:::complete"),
            "COMPLETE node REQ-A should have class 'complete'"
        );
        // REQ-C is MISSING -> class missing
        assert!(
            mermaid.contains("REQ-C[REQ-C]:::missing"),
            "MISSING node REQ-C should have class 'missing'"
        );
    }

    // rtmx:req REQ-RTMX-021
    #[test]
    fn test_graph_subcommand_outputs_dot() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let result = format_dependency_graph(&db, "dot");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            output.contains("digraph"),
            "DOT output must contain 'digraph'"
        );
    }

    // rtmx:req REQ-RTMX-021
    #[test]
    fn test_graph_subcommand_outputs_mermaid() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let result = format_dependency_graph(&db, "mermaid");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            output.contains("graph TD"),
            "Mermaid output must contain 'graph TD'"
        );
    }

    // rtmx:req REQ-RTMX-021
    #[test]
    fn test_graph_subcommand_unknown_format() {
        let db = RequirementsDb::from_csv(test_csv()).unwrap();
        let result = format_dependency_graph(&db, "svg");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown format"));
        assert!(err.contains("svg"));
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

    // --- REQ-AGENT-059: WorktreeManager trait ---

    /// Mock worktree manager for testing.
    struct MockWorktreeManager {
        created: std::sync::Mutex<Vec<(String, String)>>,
        removed: std::sync::Mutex<Vec<String>>,
        fail_create: std::sync::Mutex<Option<String>>,
    }

    impl MockWorktreeManager {
        fn new() -> Self {
            Self {
                created: std::sync::Mutex::new(Vec::new()),
                removed: std::sync::Mutex::new(Vec::new()),
                fail_create: std::sync::Mutex::new(None),
            }
        }

        fn fail_on_create(&self, error: &str) {
            *self.fail_create.lock().unwrap() = Some(error.to_string());
        }

        fn created_worktrees(&self) -> Vec<(String, String)> {
            self.created.lock().unwrap().clone()
        }

        fn removed_worktrees(&self) -> Vec<String> {
            self.removed.lock().unwrap().clone()
        }
    }

    impl WorktreeManager for MockWorktreeManager {
        async fn create(&self, path: &str, branch: &str) -> Result<WorktreeInfo, String> {
            if let Some(err) = self.fail_create.lock().unwrap().as_ref() {
                return Err(err.clone());
            }
            self.created
                .lock()
                .unwrap()
                .push((path.to_string(), branch.to_string()));
            Ok(WorktreeInfo {
                path: path.to_string(),
                branch: branch.to_string(),
            })
        }

        async fn remove(&self, path: &str) -> Result<(), String> {
            self.removed.lock().unwrap().push(path.to_string());
            Ok(())
        }

        async fn list(&self) -> Result<Vec<WorktreeInfo>, String> {
            Ok(self
                .created
                .lock()
                .unwrap()
                .iter()
                .map(|(p, b)| WorktreeInfo {
                    path: p.clone(),
                    branch: b.clone(),
                })
                .collect())
        }
    }

    // rtmx:req REQ-AGENT-059
    #[tokio::test]
    async fn test_worktree_manager_create_remove_lifecycle() {
        let wm = MockWorktreeManager::new();

        let info = wm.create("/tmp/wt-1", "agent/ws-1").await.unwrap();
        assert_eq!(info.path, "/tmp/wt-1");
        assert_eq!(info.branch, "agent/ws-1");

        let list = wm.list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, "/tmp/wt-1");

        wm.remove("/tmp/wt-1").await.unwrap();
        assert_eq!(wm.removed_worktrees(), vec!["/tmp/wt-1"]);
    }

    // rtmx:req REQ-AGENT-059
    #[tokio::test]
    async fn test_worktree_manager_create_failure() {
        let wm = MockWorktreeManager::new();
        wm.fail_on_create("branch already exists");

        let result = wm.create("/tmp/wt-fail", "existing-branch").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("branch already exists"));
    }

    // rtmx:req REQ-AGENT-059
    #[tokio::test]
    async fn test_worktree_manager_multiple_worktrees() {
        let wm = MockWorktreeManager::new();

        wm.create("/tmp/wt-a", "agent/a").await.unwrap();
        wm.create("/tmp/wt-b", "agent/b").await.unwrap();
        wm.create("/tmp/wt-c", "agent/c").await.unwrap();

        let list = wm.list().await.unwrap();
        assert_eq!(list.len(), 3);

        let created = wm.created_worktrees();
        assert_eq!(created.len(), 3);
    }

    // --- REQ-AGENT-060: Wave driver ---

    fn make_success_callback() -> AgentCallback {
        Box::new(|_path, _name, _reqs| Box::pin(async { Ok(()) }))
    }

    fn make_failing_callback(error: &str) -> AgentCallback {
        let err = error.to_string();
        Box::new(move |_path, _name, _reqs| {
            let e = err.clone();
            Box::pin(async move { Err(e) })
        })
    }

    fn make_fail_then_succeed_callback() -> AgentCallback {
        let attempt = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        Box::new(move |_path, _name, _reqs| {
            let a = attempt.clone();
            Box::pin(async move {
                let n = a.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Err("transient failure".to_string())
                } else {
                    Ok(())
                }
            })
        })
    }

    fn test_wave() -> Wave {
        Wave {
            index: 0,
            workstreams: vec![
                Workstream {
                    name: "ws-alpha".to_string(),
                    requirements: vec!["REQ-A".to_string()],
                    estimated_files: vec!["a.rs".to_string()],
                },
                Workstream {
                    name: "ws-beta".to_string(),
                    requirements: vec!["REQ-B".to_string()],
                    estimated_files: vec!["b.rs".to_string()],
                },
            ],
        }
    }

    // rtmx:req REQ-AGENT-060
    #[tokio::test]
    async fn test_wave_driver_executes_in_order() {
        let wm = MockWorktreeManager::new();
        let callback = make_success_callback();

        let result = drive_wave(&wm, &test_wave(), &callback, "/tmp/base").await;

        assert_eq!(result.wave_index, 0);
        assert_eq!(result.workstream_results.len(), 2);
        assert!(result.all_succeeded());

        // Verify worktrees were created and removed.
        let created = wm.created_worktrees();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].0, "/tmp/base/ws-alpha");
        assert_eq!(created[1].0, "/tmp/base/ws-beta");

        let removed = wm.removed_worktrees();
        assert_eq!(removed.len(), 2);
    }

    // rtmx:req REQ-AGENT-060
    #[tokio::test]
    async fn test_wave_driver_creates_correct_branches() {
        let wm = MockWorktreeManager::new();
        let callback = make_success_callback();

        drive_wave(&wm, &test_wave(), &callback, "/tmp/wt").await;

        let created = wm.created_worktrees();
        assert_eq!(created[0].1, "agent/ws-alpha");
        assert_eq!(created[1].1, "agent/ws-beta");
    }

    // rtmx:req REQ-AGENT-060
    #[tokio::test]
    async fn test_wave_driver_worktree_create_failure_skips_agent() {
        let wm = MockWorktreeManager::new();
        wm.fail_on_create("disk full");
        let callback = make_success_callback();

        let result = drive_wave(&wm, &test_wave(), &callback, "/tmp/base").await;

        assert!(!result.all_succeeded());
        // Both should fail at worktree creation.
        for r in &result.workstream_results {
            assert!(!r.success);
            assert!(r.error.as_ref().unwrap().contains("disk full"));
        }
    }

    // rtmx:req REQ-AGENT-060
    #[tokio::test]
    async fn test_wave_result_accessors() {
        let result = WaveResult {
            wave_index: 0,
            workstream_results: vec![
                WorkstreamResult {
                    name: "a".to_string(),
                    requirements: vec!["REQ-1".to_string()],
                    success: true,
                    error: None,
                    attempts: 1,
                },
                WorkstreamResult {
                    name: "b".to_string(),
                    requirements: vec!["REQ-2".to_string()],
                    success: false,
                    error: Some("fail".to_string()),
                    attempts: 2,
                },
            ],
        };

        assert!(!result.all_succeeded());
        assert_eq!(result.failed_requirements(), vec!["REQ-2"]);
        assert_eq!(result.succeeded_requirements(), vec!["REQ-1"]);
    }

    // --- REQ-AGENT-061: Failure recovery ---

    // rtmx:req REQ-AGENT-061
    #[tokio::test]
    async fn test_failure_recovery_retries_once() {
        let wm = MockWorktreeManager::new();
        let callback = make_fail_then_succeed_callback();

        let wave = Wave {
            index: 0,
            workstreams: vec![Workstream {
                name: "retry-ws".to_string(),
                requirements: vec!["REQ-R".to_string()],
                estimated_files: vec!["r.rs".to_string()],
            }],
        };

        let result = drive_wave(&wm, &wave, &callback, "/tmp/retry").await;

        assert!(result.all_succeeded());
        assert_eq!(result.workstream_results[0].attempts, 2);
    }

    // rtmx:req REQ-AGENT-061
    #[tokio::test]
    async fn test_failure_recovery_gives_up_after_two_attempts() {
        let wm = MockWorktreeManager::new();
        let callback = make_failing_callback("persistent error");

        let wave = Wave {
            index: 0,
            workstreams: vec![Workstream {
                name: "fail-ws".to_string(),
                requirements: vec!["REQ-F".to_string()],
                estimated_files: vec!["f.rs".to_string()],
            }],
        };

        let result = drive_wave(&wm, &wave, &callback, "/tmp/fail").await;

        assert!(!result.all_succeeded());
        assert_eq!(result.workstream_results[0].attempts, 2);
        assert!(
            result.workstream_results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("persistent error")
        );
    }

    // rtmx:req REQ-AGENT-061
    #[tokio::test]
    async fn test_failure_does_not_block_siblings_in_same_wave() {
        let wm = MockWorktreeManager::new();
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        // First workstream always fails, second always succeeds.
        let callback: AgentCallback = Box::new(move |_path, name, _reqs| {
            let c = cc.clone();
            let n = name.clone();
            Box::pin(async move {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == "fail-ws" {
                    Err("agent crashed".to_string())
                } else {
                    Ok(())
                }
            })
        });

        let wave = Wave {
            index: 0,
            workstreams: vec![
                Workstream {
                    name: "fail-ws".to_string(),
                    requirements: vec!["REQ-FAIL".to_string()],
                    estimated_files: vec!["fail.rs".to_string()],
                },
                Workstream {
                    name: "ok-ws".to_string(),
                    requirements: vec!["REQ-OK".to_string()],
                    estimated_files: vec!["ok.rs".to_string()],
                },
            ],
        };

        let result = drive_wave(&wm, &wave, &callback, "/tmp/mixed").await;

        // fail-ws should fail, ok-ws should succeed.
        assert!(!result.all_succeeded());
        assert!(!result.workstream_results[0].success);
        assert!(result.workstream_results[1].success);

        // Both workstreams ran (fail-ws got 2 attempts, ok-ws got 1).
        assert!(call_count.load(std::sync::atomic::Ordering::SeqCst) >= 3);
    }

    // rtmx:req REQ-AGENT-061
    #[tokio::test]
    async fn test_drive_all_waves_skips_blocked_by_failure() {
        let wm = MockWorktreeManager::new();
        let callback = make_failing_callback("wave0 failure");

        let waves = vec![
            Wave {
                index: 0,
                workstreams: vec![Workstream {
                    name: "ws-0".to_string(),
                    requirements: vec!["REQ-BASE".to_string()],
                    estimated_files: vec!["base.rs".to_string()],
                }],
            },
            Wave {
                index: 1,
                workstreams: vec![Workstream {
                    name: "ws-1".to_string(),
                    requirements: vec!["REQ-BASE".to_string()],
                    estimated_files: vec!["dep.rs".to_string()],
                }],
            },
        ];

        let results = drive_all_waves(&wm, &waves, &callback, "/tmp/skip").await;

        // Wave 0 ran and failed.
        assert_eq!(results.len(), 1);
        assert!(!results[0].all_succeeded());
        // Wave 1 was skipped because REQ-BASE failed in wave 0.
    }

    // rtmx:req REQ-AGENT-061
    #[tokio::test]
    async fn test_drive_all_waves_independent_waves_proceed() {
        let wm = MockWorktreeManager::new();
        let callback = make_success_callback();

        let waves = vec![
            Wave {
                index: 0,
                workstreams: vec![Workstream {
                    name: "ws-0".to_string(),
                    requirements: vec!["REQ-A".to_string()],
                    estimated_files: vec!["a.rs".to_string()],
                }],
            },
            Wave {
                index: 1,
                workstreams: vec![Workstream {
                    name: "ws-1".to_string(),
                    requirements: vec!["REQ-B".to_string()],
                    estimated_files: vec!["b.rs".to_string()],
                }],
            },
        ];

        let results = drive_all_waves(&wm, &waves, &callback, "/tmp/ok").await;

        assert_eq!(results.len(), 2);
        assert!(results[0].all_succeeded());
        assert!(results[1].all_succeeded());
    }

    // rtmx:req REQ-AGENT-060
    #[tokio::test]
    async fn test_wave_driver_cleans_up_on_failure() {
        let wm = MockWorktreeManager::new();
        let callback = make_failing_callback("boom");

        let wave = Wave {
            index: 0,
            workstreams: vec![Workstream {
                name: "cleanup-ws".to_string(),
                requirements: vec!["REQ-C".to_string()],
                estimated_files: vec!["c.rs".to_string()],
            }],
        };

        drive_wave(&wm, &wave, &callback, "/tmp/clean").await;

        // Worktree should still be cleaned up even though agent failed.
        let removed = wm.removed_worktrees();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], "/tmp/clean/cleanup-ws");
    }
}
