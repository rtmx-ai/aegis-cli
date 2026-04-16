//! RTMX requirement types and CSV parser.
//!
//! Reads requirements from .rtmx/database.csv and exposes them
//! as queryable domain objects for the agent loop.

use crate::error::DomainError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// A single RTMX requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub req_id: String,
    pub category: String,
    pub subcategory: String,
    pub requirement_text: String,
    pub target_value: String,
    pub test_module: String,
    pub test_function: String,
    pub validation_method: String,
    pub status: String,
    pub priority: String,
    pub phase: String,
    pub notes: String,
    #[serde(default)]
    pub effort_weeks: String,
    #[serde(default)]
    pub dependencies: String,
    #[serde(default)]
    pub blocks: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub sprint: String,
    #[serde(default)]
    pub started_date: String,
    #[serde(default)]
    pub completed_date: String,
}

impl Requirement {
    /// Parse the pipe-delimited dependencies field into a list of requirement IDs.
    pub fn dependency_ids(&self) -> Vec<&str> {
        if self.dependencies.trim().is_empty() {
            Vec::new()
        } else {
            self.dependencies.split('|').map(|s| s.trim()).collect()
        }
    }

    /// Parse the pipe-delimited blocks field into a list of requirement IDs.
    pub fn blocks_ids(&self) -> Vec<&str> {
        if self.blocks.trim().is_empty() {
            Vec::new()
        } else {
            self.blocks.split('|').map(|s| s.trim()).collect()
        }
    }
}

/// A parsed RTMX requirements database.
#[derive(Debug, Clone)]
pub struct RequirementsDb {
    requirements: Vec<Requirement>,
    by_id: HashMap<String, usize>,
}

impl RequirementsDb {
    /// Parse requirements from a CSV string.
    pub fn from_csv(csv_content: &str) -> Result<Self, DomainError> {
        let mut requirements = Vec::new();
        let mut lines = csv_content.lines();

        // Parse header
        let header = lines
            .next()
            .ok_or_else(|| DomainError::Other("Empty CSV file".to_string()))?;
        let columns: Vec<&str> = parse_csv_row(header);

        let col_index = |name: &str| -> Option<usize> { columns.iter().position(|c| *c == name) };

        let id_col = col_index("req_id")
            .ok_or_else(|| DomainError::Other("Missing req_id column".to_string()))?;

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let fields = parse_csv_row(line);
            if fields.len() <= id_col {
                continue;
            }

            let get = |name: &str| -> String {
                col_index(name)
                    .and_then(|i| fields.get(i))
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            };

            requirements.push(Requirement {
                req_id: get("req_id"),
                category: get("category"),
                subcategory: get("subcategory"),
                requirement_text: get("requirement_text"),
                target_value: get("target_value"),
                test_module: get("test_module"),
                test_function: get("test_function"),
                validation_method: get("validation_method"),
                status: get("status"),
                priority: get("priority"),
                phase: get("phase"),
                notes: get("notes"),
                effort_weeks: get("effort_weeks"),
                dependencies: get("dependencies"),
                blocks: get("blocks"),
                assignee: get("assignee"),
                sprint: get("sprint"),
                started_date: get("started_date"),
                completed_date: get("completed_date"),
            });
        }

        let by_id: HashMap<String, usize> = requirements
            .iter()
            .enumerate()
            .map(|(i, r)| (r.req_id.clone(), i))
            .collect();

        Ok(Self {
            requirements,
            by_id,
        })
    }

    /// Load requirements from a CSV file path.
    pub fn load(path: &Path) -> Result<Self, DomainError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| DomainError::Other(format!("Failed to read {}: {e}", path.display())))?;
        Self::from_csv(&content)
    }

    /// Get a requirement by ID.
    pub fn get(&self, req_id: &str) -> Option<&Requirement> {
        self.by_id.get(req_id).map(|&i| &self.requirements[i])
    }

    /// Get all requirements.
    pub fn all(&self) -> &[Requirement] {
        &self.requirements
    }

    /// Get requirements by category.
    pub fn by_category(&self, category: &str) -> Vec<&Requirement> {
        self.requirements
            .iter()
            .filter(|r| r.category == category)
            .collect()
    }

    /// Get requirements by status.
    pub fn by_status(&self, status: &str) -> Vec<&Requirement> {
        self.requirements
            .iter()
            .filter(|r| r.status == status)
            .collect()
    }

    /// Count total requirements.
    pub fn count(&self) -> usize {
        self.requirements.len()
    }

    /// Count requirements by status.
    pub fn count_by_status(&self, status: &str) -> usize {
        self.requirements
            .iter()
            .filter(|r| r.status == status)
            .count()
    }

    /// Get a mutable reference to a requirement by ID.
    fn get_mut(&mut self, req_id: &str) -> Result<&mut Requirement, DomainError> {
        let &idx = self
            .by_id
            .get(req_id)
            .ok_or_else(|| DomainError::RequirementNotFound {
                id: req_id.to_string(),
            })?;
        Ok(&mut self.requirements[idx])
    }

    /// Update the status field for a requirement.
    pub fn update_status(&mut self, req_id: &str, new_status: &str) -> Result<(), DomainError> {
        let req = self.get_mut(req_id)?;
        req.status = new_status.to_string();
        Ok(())
    }

    /// Update the test_module and test_function fields for a requirement.
    pub fn update_test_info(
        &mut self,
        req_id: &str,
        test_module: &str,
        test_function: &str,
    ) -> Result<(), DomainError> {
        let req = self.get_mut(req_id)?;
        req.test_module = test_module.to_string();
        req.test_function = test_function.to_string();
        Ok(())
    }

    /// Set a requirement to COMPLETE with today's date.
    pub fn set_completed(&mut self, req_id: &str) -> Result<(), DomainError> {
        let req = self.get_mut(req_id)?;
        req.status = "COMPLETE".to_string();
        req.completed_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        Ok(())
    }

    /// Write the current state back to a CSV file.
    pub fn save_csv(&self, path: &Path) -> Result<(), DomainError> {
        if let Some(parent) = path.parent().filter(|p| !p.exists()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                DomainError::Other(format!(
                    "Failed to create directory {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let mut out = String::new();
        out.push_str(CSV_HEADER);
        out.push('\n');

        for req in &self.requirements {
            let row = [
                &req.req_id,
                &req.category,
                &req.subcategory,
                &req.requirement_text,
                &req.target_value,
                &req.test_module,
                &req.test_function,
                &req.validation_method,
                &req.status,
                &req.priority,
                &req.phase,
                &req.notes,
                &req.effort_weeks,
                &req.dependencies,
                &req.blocks,
                &req.assignee,
                &req.sprint,
                &req.started_date,
                &req.completed_date,
            ];
            let formatted: Vec<String> = row
                .iter()
                .map(|f| {
                    if f.contains(',') || f.contains('"') {
                        format!("\"{}\"", f.replace('"', "\"\""))
                    } else {
                        f.to_string()
                    }
                })
                .collect();
            out.push_str(&formatted.join(","));
            out.push('\n');
        }

        std::fs::write(path, out)
            .map_err(|e| DomainError::Other(format!("Failed to write {}: {e}", path.display())))
    }
}

// ---------------------------------------------------------------------------
// REQ-RTMX-011: Dependency graph as directed acyclic graph
// ---------------------------------------------------------------------------

/// Directed acyclic graph of requirement dependencies.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Adjacency list: req_id -> set of requirement IDs it depends on.
    pub edges: HashMap<String, HashSet<String>>,
    /// Reverse adjacency: req_id -> set of requirements that depend on it.
    pub reverse_edges: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    /// Build the DAG from all requirements in the database.
    pub fn from_db(db: &RequirementsDb) -> Self {
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        let mut reverse_edges: HashMap<String, HashSet<String>> = HashMap::new();

        for req in db.all() {
            // Ensure every requirement has an entry even if it has no deps.
            edges.entry(req.req_id.clone()).or_default();
            reverse_edges.entry(req.req_id.clone()).or_default();

            for dep_id in req.dependency_ids() {
                let dep = dep_id.to_string();
                edges
                    .entry(req.req_id.clone())
                    .or_default()
                    .insert(dep.clone());
                reverse_edges
                    .entry(dep)
                    .or_default()
                    .insert(req.req_id.clone());
            }
        }

        Self {
            edges,
            reverse_edges,
        }
    }

    /// Return requirement IDs that depend on the given requirement.
    pub fn dependents(&self, req_id: &str) -> Vec<&str> {
        self.reverse_edges
            .get(req_id)
            .map(|s| s.iter().map(|id| id.as_str()).collect())
            .unwrap_or_default()
    }

    /// Return requirement IDs that the given requirement depends on.
    pub fn dependencies(&self, req_id: &str) -> Vec<&str> {
        self.edges
            .get(req_id)
            .map(|s| s.iter().map(|id| id.as_str()).collect())
            .unwrap_or_default()
    }

    /// Topological sort via Kahn's algorithm.
    ///
    /// Returns `Ok(ordered)` with requirements in dependency order (dependencies
    /// before dependents), or `Err(cycle_members)` listing the requirement IDs
    /// involved in a cycle.
    pub fn topological_order(&self) -> Result<Vec<String>, Vec<String>> {
        // In-degree: how many dependencies does each node have?
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for (node, deps) in &self.edges {
            in_degree.entry(node.as_str()).or_insert(0);
            // Only count edges to nodes that actually exist in the graph.
            let valid_deps = deps.iter().filter(|d| self.edges.contains_key(d.as_str()));
            for dep in valid_deps {
                // dep is depended-upon, so this edge goes dep -> node in topo terms.
                // in_degree tracks how many unsatisfied deps each node has.
                *in_degree.entry(node.as_str()).or_insert(0) += 0; // ensure node exists
                let _ = in_degree.entry(dep.as_str()).or_insert(0);
            }
        }

        // Recompute properly: in_degree[node] = number of valid deps for node.
        for (node, deps) in &self.edges {
            let count = deps
                .iter()
                .filter(|d| self.edges.contains_key(d.as_str()))
                .count();
            in_degree.insert(node.as_str(), count);
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(&node, _)| node)
            .collect();

        let mut result: Vec<String> = Vec::new();

        while let Some(node) = queue.pop_front() {
            result.push(node.to_string());

            // For each requirement that depends on `node`, decrement in-degree.
            if let Some(dependents) = self.reverse_edges.get(node) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep.as_str())
                        && *deg > 0
                    {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep.as_str());
                        }
                    }
                }
            }
        }

        if result.len() == self.edges.len() {
            Ok(result)
        } else {
            // Nodes not in result are involved in a cycle.
            let in_result: HashSet<&str> = result.iter().map(|s| s.as_str()).collect();
            let cycle_members: Vec<String> = self
                .edges
                .keys()
                .filter(|k| !in_result.contains(k.as_str()))
                .cloned()
                .collect();
            Err(cycle_members)
        }
    }

    /// Returns true if the dependency graph is a valid DAG (no cycles).
    pub fn is_dag(&self) -> bool {
        self.topological_order().is_ok()
    }

    // -----------------------------------------------------------------------
    // REQ-RTMX-012: Cycle detection via Tarjan's strongly connected components
    // -----------------------------------------------------------------------

    /// Tarjan's strongly connected components algorithm (iterative to avoid
    /// stack overflow on large graphs).
    ///
    /// Returns every SCC as a Vec of requirement ids. Singleton SCCs without
    /// self-loops are *not* cycles; use [`DependencyGraph::cycles`] to filter.
    pub fn strongly_connected_components(&self) -> Vec<Vec<String>> {
        // Build a stable numeric indexing over the string-keyed edge map.
        // We include every node that appears as either a source or a target,
        // sorted for determinism so repeated runs produce matching output.
        let mut node_set: HashSet<&str> = HashSet::new();
        for (from, targets) in &self.edges {
            node_set.insert(from.as_str());
            for t in targets {
                node_set.insert(t.as_str());
            }
        }
        let mut nodes: Vec<&str> = node_set.into_iter().collect();
        nodes.sort();
        let n = nodes.len();
        let node_idx: HashMap<&str, usize> =
            nodes.iter().enumerate().map(|(i, s)| (*s, i)).collect();

        // Adjacency list as indices for the algorithm's hot loop.
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (from, targets) in &self.edges {
            if let Some(&fi) = node_idx.get(from.as_str()) {
                for t in targets {
                    if let Some(&ti) = node_idx.get(t.as_str()) {
                        adj[fi].push(ti);
                    }
                }
            }
        }

        let mut index_counter: usize = 0;
        let mut stack: Vec<usize> = Vec::new();
        let mut on_stack = vec![false; n];
        let mut indices: Vec<Option<usize>> = vec![None; n];
        let mut lowlinks = vec![0usize; n];
        let mut result: Vec<Vec<String>> = Vec::new();

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
                        Some(_) => {
                            // w is in a previously-closed SCC -- ignore.
                        }
                    }
                } else {
                    work.pop();
                    if indices[v] == Some(lowlinks[v]) {
                        let mut component = Vec::new();
                        loop {
                            let w = stack.pop().expect("stack non-empty during SCC pop");
                            on_stack[w] = false;
                            component.push(nodes[w].to_string());
                            if w == v {
                                break;
                            }
                        }
                        result.push(component);
                    }
                    if let Some((parent, _)) = work.last().copied()
                        && lowlinks[v] < lowlinks[parent]
                    {
                        lowlinks[parent] = lowlinks[v];
                    }
                }
            }
        }

        result
    }

    /// Return only the SCCs that represent actual cycles: size > 1, or
    /// singletons with a self-loop.
    pub fn cycles(&self) -> Vec<Vec<String>> {
        self.strongly_connected_components()
            .into_iter()
            .filter(|scc| {
                if scc.len() > 1 {
                    return true;
                }
                // Singleton -- check for self-loop in the string-keyed edges.
                if let Some(id) = scc.first() {
                    return self.edges.get(id).map(|s| s.contains(id)).unwrap_or(false);
                }
                false
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // REQ-RTMX-007: Priority and critical-path analysis
    // -----------------------------------------------------------------------

    /// Count requirements that transitively depend on `req_id`.
    ///
    /// Walks the `reverse_edges` graph (downstream direction) with BFS and
    /// returns the number of distinct descendants excluding `req_id` itself.
    /// This is "how much work would be unblocked if this requirement were
    /// completed" -- the leverage metric used by [`priority_scores`].
    ///
    /// [`priority_scores`]: DependencyGraph::priority_scores
    pub fn transitive_blocks(&self, req_id: &str) -> usize {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        queue.push_back(req_id);
        visited.insert(req_id);

        while let Some(node) = queue.pop_front() {
            if let Some(dependents) = self.reverse_edges.get(node) {
                for dep in dependents {
                    if visited.insert(dep.as_str()) {
                        queue.push_back(dep.as_str());
                    }
                }
            }
        }

        // Exclude the starting node from the count.
        visited.len().saturating_sub(1)
    }

    /// Compute prioritization scores for all MISSING requirements whose
    /// dependencies are satisfied (the actionable frontier).
    ///
    /// A dependency is satisfied when every entry in the requirement's
    /// `dependencies` column refers to a requirement that is COMPLETE (or
    /// DONE). Requirements with unknown or incomplete dependencies are not
    /// actionable yet and are omitted from the result.
    pub fn priority_scores(&self, db: &RequirementsDb) -> Vec<PriorityScore> {
        let mut scores = Vec::new();

        for req in db.all() {
            if !is_missing(&req.status) {
                continue;
            }
            if !deps_satisfied(req, db) {
                continue;
            }

            let direct_blocks = self
                .reverse_edges
                .get(&req.req_id)
                .map(|s| s.len())
                .unwrap_or(0);
            let transitive_blocks = self.transitive_blocks(&req.req_id);
            let weight = priority_weight(&req.priority);
            let score = weight * (1.0 + transitive_blocks as f64);
            let effort_weeks = parse_effort(&req.effort_weeks);

            scores.push(PriorityScore {
                req_id: req.req_id.clone(),
                direct_blocks,
                transitive_blocks,
                score,
                effort_weeks,
            });
        }

        scores
    }

    /// The critical path: actionable requirements ordered by score
    /// (highest first). Ties are broken by transitive block count then by
    /// req_id for deterministic output.
    pub fn critical_path(&self, db: &RequirementsDb) -> Vec<PriorityScore> {
        let mut scores = self.priority_scores(db);
        scores.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.transitive_blocks.cmp(&a.transitive_blocks))
                .then_with(|| a.req_id.cmp(&b.req_id))
        });
        scores
    }

    // -----------------------------------------------------------------------
    // REQ-RTMX-008: Requirement conflict detection
    // -----------------------------------------------------------------------

    /// Detect logical conflicts in the requirement set:
    ///
    /// - [`RequirementConflict::CircularDependency`] for each cycle
    ///   reported by [`Self::cycles`].
    /// - [`RequirementConflict::DanglingDependency`] when a requirement
    ///   declares a dependency on a req_id not present in the database.
    /// - [`RequirementConflict::DanglingBlocks`] when a requirement's
    ///   `blocks` column references a req_id not present in the database.
    /// - [`RequirementConflict::ContradictoryEdge`] when A depends on B
    ///   *and* B declares it blocks A -- a logical impossibility ("I need
    ///   B before I run" paired with "B prevents A from running").
    pub fn detect_conflicts(&self, db: &RequirementsDb) -> Vec<RequirementConflict> {
        let mut conflicts: Vec<RequirementConflict> = Vec::new();

        // Circular dependencies via the existing Tarjan-based cycles().
        // Sort each cycle and the outer list for deterministic output.
        let mut cycles = self.cycles();
        for scc in cycles.iter_mut() {
            scc.sort();
        }
        cycles.sort_by(|a, b| a.first().cmp(&b.first()));
        for members in cycles {
            conflicts.push(RequirementConflict::CircularDependency { members });
        }

        // Dangling dependencies and blocks. Iterate in database order so
        // conflicts appear in a predictable sequence.
        for req in db.all() {
            for dep in req.dependency_ids() {
                if dep.is_empty() {
                    continue;
                }
                if db.get(dep).is_none() {
                    conflicts.push(RequirementConflict::DanglingDependency {
                        req_id: req.req_id.clone(),
                        missing_dep: dep.to_string(),
                    });
                }
            }
            for target in req.blocks_ids() {
                if target.is_empty() {
                    continue;
                }
                if db.get(target).is_none() {
                    conflicts.push(RequirementConflict::DanglingBlocks {
                        req_id: req.req_id.clone(),
                        missing_target: target.to_string(),
                    });
                }
            }
        }

        // Contradictory edges: A depends on B and B blocks A. We iterate
        // over every dep edge A -> B and check B's blocks list for A.
        // To avoid double-reporting the same unordered pair, emit only
        // when req_a < req_b lexicographically.
        let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
        for req in db.all() {
            for dep in req.dependency_ids() {
                if dep.is_empty() {
                    continue;
                }
                let Some(dep_req) = db.get(dep) else {
                    continue;
                };
                let blocks_list = dep_req.blocks_ids();
                if blocks_list.contains(&req.req_id.as_str()) {
                    let (a, b) = if req.req_id < dep_req.req_id {
                        (req.req_id.clone(), dep_req.req_id.clone())
                    } else {
                        (dep_req.req_id.clone(), req.req_id.clone())
                    };
                    if seen_pairs.insert((a.clone(), b.clone())) {
                        conflicts.push(RequirementConflict::ContradictoryEdge {
                            req_a: a,
                            req_b: b,
                            reason: format!(
                                "{} depends on {}, but {} lists {} in its blocks column",
                                req.req_id, dep_req.req_id, dep_req.req_id, req.req_id
                            ),
                        });
                    }
                }
            }
        }

        conflicts
    }

    // -----------------------------------------------------------------------
    // REQ-RTMX-013: Graph rendering (Graphviz DOT and Mermaid flowchart)
    // -----------------------------------------------------------------------

    /// Render the graph as a Graphviz DOT document (unstyled).
    ///
    /// Example:
    /// ```text
    /// digraph deps {
    ///     "REQ-A" -> "REQ-B";
    /// }
    /// ```
    pub fn to_dot(&self) -> String {
        let mut out = String::from("digraph deps {\n");
        // Emit nodes (sorted for deterministic output) so isolated nodes
        // still appear.
        let mut node_ids: Vec<&String> = self.edges.keys().collect();
        node_ids.sort();
        for id in &node_ids {
            out.push_str(&format!("    \"{}\";\n", escape_dot(id)));
        }
        let mut edge_pairs: Vec<(&String, &String)> = Vec::new();
        for (from, targets) in &self.edges {
            for to in targets {
                edge_pairs.push((from, to));
            }
        }
        edge_pairs.sort();
        for (from, to) in edge_pairs {
            out.push_str(&format!(
                "    \"{}\" -> \"{}\";\n",
                escape_dot(from),
                escape_dot(to),
            ));
        }
        out.push_str("}\n");
        out
    }

    /// Render the graph as a Graphviz DOT document with nodes coloured by
    /// requirement status. COMPLETE -> green, MISSING -> yellow, anything
    /// containing "BLOCK" (case-insensitive) -> red, otherwise uncoloured.
    pub fn to_dot_styled(&self, db: &RequirementsDb) -> String {
        let mut out = String::from("digraph deps {\n");
        out.push_str("    node [style=filled];\n");
        let mut node_ids: Vec<&String> = self.edges.keys().collect();
        node_ids.sort();
        for id in &node_ids {
            let color = status_color(db.get(id).map(|r| r.status.as_str()));
            match color {
                Some(c) => out.push_str(&format!(
                    "    \"{}\" [fillcolor=\"{}\"];\n",
                    escape_dot(id),
                    c
                )),
                None => out.push_str(&format!("    \"{}\";\n", escape_dot(id))),
            }
        }
        let mut edge_pairs: Vec<(&String, &String)> = Vec::new();
        for (from, targets) in &self.edges {
            for to in targets {
                edge_pairs.push((from, to));
            }
        }
        edge_pairs.sort();
        for (from, to) in edge_pairs {
            out.push_str(&format!(
                "    \"{}\" -> \"{}\";\n",
                escape_dot(from),
                escape_dot(to),
            ));
        }
        out.push_str("}\n");
        out
    }

    /// Render the graph as a Mermaid flowchart (top-down, unstyled).
    ///
    /// Example:
    /// ```text
    /// graph TD
    ///     REQ-A --> REQ-B
    /// ```
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("graph TD\n");
        let mut node_ids: Vec<&String> = self.edges.keys().collect();
        node_ids.sort();
        for id in &node_ids {
            out.push_str(&format!("    {}\n", mermaid_node_decl(id)));
        }
        let mut edge_pairs: Vec<(&String, &String)> = Vec::new();
        for (from, targets) in &self.edges {
            for to in targets {
                edge_pairs.push((from, to));
            }
        }
        edge_pairs.sort();
        for (from, to) in edge_pairs {
            out.push_str(&format!(
                "    {} --> {}\n",
                mermaid_id(from),
                mermaid_id(to),
            ));
        }
        out
    }

    /// Render the graph as a Mermaid flowchart with per-status node styling
    /// via `classDef`. Nodes whose status cannot be resolved are left with
    /// default styling.
    pub fn to_mermaid_styled(&self, db: &RequirementsDb) -> String {
        let mut out = String::from("graph TD\n");
        out.push_str("    classDef complete fill:#9f9,stroke:green,color:#000;\n");
        out.push_str("    classDef missing fill:#ff9,stroke:#cc0,color:#000;\n");
        out.push_str("    classDef blocked fill:#f99,stroke:red,color:#000;\n");

        let mut node_ids: Vec<&String> = self.edges.keys().collect();
        node_ids.sort();
        for id in &node_ids {
            out.push_str(&format!("    {}\n", mermaid_node_decl(id)));
        }
        let mut edge_pairs: Vec<(&String, &String)> = Vec::new();
        for (from, targets) in &self.edges {
            for to in targets {
                edge_pairs.push((from, to));
            }
        }
        edge_pairs.sort();
        for (from, to) in edge_pairs {
            out.push_str(&format!(
                "    {} --> {}\n",
                mermaid_id(from),
                mermaid_id(to),
            ));
        }
        // class assignments (sorted for stable output)
        for id in &node_ids {
            if let Some(class) = status_mermaid_class(db.get(id).map(|r| r.status.as_str())) {
                out.push_str(&format!("    class {} {};\n", mermaid_id(id), class));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// REQ-RTMX-007: Priority score and helpers
// ---------------------------------------------------------------------------

/// A score describing how much downstream work a requirement unblocks.
///
/// Produced by [`DependencyGraph::priority_scores`] and
/// [`DependencyGraph::critical_path`]. `score` is
/// `priority_weight(priority) * (1 + transitive_blocks)` so that a
/// HIGH-priority requirement with many dependents dominates a LOW-priority
/// leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorityScore {
    pub req_id: String,
    /// Number of requirements directly depending on this one.
    pub direct_blocks: usize,
    /// Number of requirements transitively depending on this one.
    pub transitive_blocks: usize,
    /// Combined score: priority weight times (1 + transitive_blocks).
    pub score: f64,
    /// Effort in weeks parsed from the CSV (0.0 if absent or unparseable).
    pub effort_weeks: f64,
}

/// Map a priority string (case-insensitive) to a numeric weight used by
/// [`DependencyGraph::priority_scores`]. HIGH/CRITICAL=3.0, MEDIUM=2.0,
/// LOW=1.0, unknown=1.0.
fn priority_weight(priority: &str) -> f64 {
    match priority.trim().to_ascii_uppercase().as_str() {
        "HIGH" | "CRITICAL" => 3.0,
        "MEDIUM" => 2.0,
        "LOW" => 1.0,
        _ => 1.0,
    }
}

/// Parse the `effort_weeks` CSV column. Empty or non-numeric values map
/// to 0.0 so the caller does not have to handle Result everywhere.
fn parse_effort(effort: &str) -> f64 {
    effort.trim().parse::<f64>().unwrap_or(0.0)
}

/// Return true if a status string denotes an unstarted / missing item.
fn is_missing(status: &str) -> bool {
    matches!(status.trim().to_ascii_uppercase().as_str(), "MISSING")
}

/// Return true if a status string denotes completion (COMPLETE or DONE).
fn is_complete(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_uppercase().as_str(),
        "COMPLETE" | "DONE"
    )
}

/// A requirement is actionable when every declared dependency resolves to
/// a COMPLETE requirement in the database. Missing or in-progress deps
/// mean the requirement is not yet on the frontier.
fn deps_satisfied(req: &Requirement, db: &RequirementsDb) -> bool {
    for dep in req.dependency_ids() {
        if dep.is_empty() {
            continue;
        }
        match db.get(dep) {
            Some(r) if is_complete(&r.status) => continue,
            _ => return false,
        }
    }
    true
}

// ---------------------------------------------------------------------------
// REQ-RTMX-008: Requirement conflicts
// ---------------------------------------------------------------------------

/// A logical conflict discovered in the requirement set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementConflict {
    /// A set of requirements that form a cycle in the dependency graph.
    CircularDependency { members: Vec<String> },
    /// A depends on B *and* B declares it blocks A: contradictory edge.
    ContradictoryEdge {
        req_a: String,
        req_b: String,
        reason: String,
    },
    /// A requirement declares a dependency on a req_id not present in
    /// the database.
    DanglingDependency { req_id: String, missing_dep: String },
    /// A requirement's `blocks` column names a req_id not present in
    /// the database.
    DanglingBlocks {
        req_id: String,
        missing_target: String,
    },
}

// ---------------------------------------------------------------------------
// REQ-RTMX-004: Test marker scanning
// ---------------------------------------------------------------------------

/// A discovered test-to-requirement marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerScanResult {
    pub req_id: String,
    pub file_path: String,
    pub function_name: Option<String>,
    pub line_number: usize,
}

/// Walk `.rs` files under `source_dir` and scan for requirement markers.
///
/// Recognized formats:
/// - `// rtmx:req REQ-XXX-NNN`
/// - `// @req REQ-XXX-NNN`
/// - `#[req(REQ-XXX-NNN)]`
pub fn scan_markers(source_dir: &Path) -> Vec<MarkerScanResult> {
    let re = Regex::new(
        r"(?://\s*rtmx:req\s+(REQ-[A-Z]+-\d+))|(?://\s*@req\s+(REQ-[A-Z]+-\d+))|(?:#\[req\((REQ-[A-Z]+-\d+)\)\])",
    )
    .expect("marker regex is valid");

    let fn_re = Regex::new(r"\bfn\s+(\w+)").expect("fn regex is valid");

    let mut results = Vec::new();
    let mut files = Vec::new();
    collect_rs_files(source_dir, &mut files);

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if let Some(caps) = re.captures(line) {
                let req_id = caps
                    .get(1)
                    .or_else(|| caps.get(2))
                    .or_else(|| caps.get(3))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                // Look forward from the marker to find the nearest fn declaration.
                let function_name = lines[i..]
                    .iter()
                    .take(10)
                    .find_map(|l| fn_re.captures(l).map(|c| c[1].to_string()));

                results.push(MarkerScanResult {
                    req_id,
                    file_path: file_path.to_string_lossy().to_string(),
                    function_name,
                    line_number: i + 1,
                });
            }
        }
    }

    results
}

/// Recursively collect `.rs` files under a directory.
fn collect_rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

// ---------------------------------------------------------------------------
// REQ-RTMX-003: Closed-loop verification
// ---------------------------------------------------------------------------

/// Outcome of verifying a requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationOutcome {
    /// The requirement has a linked test and the test file exists.
    Passed,
    /// The requirement has a linked test but something is wrong.
    Failed { reason: String },
    /// The requirement has no test_module or test_function linked.
    NoTestLinked,
}

/// Result of verifying a single requirement.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub req_id: String,
    pub outcome: VerificationOutcome,
    pub test_module: Option<String>,
    pub test_function: Option<String>,
}

/// Verify that a requirement has linked tests and the test file exists.
///
/// Actual test execution is out of scope for the domain layer -- the agent
/// is responsible for running `cargo test`. This function checks that the
/// metadata links are present and the referenced file exists on disk.
pub fn verify_requirement(db: &RequirementsDb, req_id: &str) -> VerificationResult {
    let req = match db.get(req_id) {
        Some(r) => r,
        None => {
            return VerificationResult {
                req_id: req_id.to_string(),
                outcome: VerificationOutcome::Failed {
                    reason: format!("Requirement {req_id} not found in database"),
                },
                test_module: None,
                test_function: None,
            };
        }
    };

    if req.test_module.trim().is_empty() || req.test_function.trim().is_empty() {
        return VerificationResult {
            req_id: req_id.to_string(),
            outcome: VerificationOutcome::NoTestLinked,
            test_module: None,
            test_function: None,
        };
    }

    let test_path = Path::new(&req.test_module);
    if !test_path.exists() {
        return VerificationResult {
            req_id: req_id.to_string(),
            outcome: VerificationOutcome::Failed {
                reason: format!("Test file not found: {}", req.test_module),
            },
            test_module: Some(req.test_module.clone()),
            test_function: Some(req.test_function.clone()),
        };
    }

    VerificationResult {
        req_id: req_id.to_string(),
        outcome: VerificationOutcome::Passed,
        test_module: Some(req.test_module.clone()),
        test_function: Some(req.test_function.clone()),
    }
}

/// CSV header matching the full RTMX database schema.
const CSV_HEADER: &str = "req_id,category,subcategory,requirement_text,\
    target_value,test_module,test_function,validation_method,status,\
    priority,phase,notes,effort_weeks,dependencies,blocks,assignee,\
    sprint,started_date,completed_date";

/// Simple CSV row parser that handles quoted fields with commas.
fn parse_csv_row(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let bytes = line.as_bytes();

    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b',' if !in_quotes => {
                let field = &line[start..i];
                fields.push(field.trim_matches('"'));
                start = i + 1;
            }
            _ => {}
        }
    }
    // Last field
    let field = &line[start..];
    fields.push(field.trim_matches('"'));

    fields
}

/// Escape a node identifier for inclusion inside a DOT quoted string.
fn escape_dot(id: &str) -> String {
    id.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Render a Mermaid node id. Mermaid tolerates letters, digits, `-`, and `_`
/// in bare identifiers; any other character forces a quoted label form.
fn mermaid_id(id: &str) -> String {
    if id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        id.to_string()
    } else {
        // Use the bracketed label form to be safe: `Nxx["actual-id"]`. We
        // stabilise the bracketed id by hashing positionally, but for our
        // domain (REQ-XXX-NNN) this branch is unreachable in practice.
        format!("n{}[\"{}\"]", id.len(), id.replace('"', "&quot;"))
    }
}

/// Render a standalone Mermaid node declaration (id with optional label).
fn mermaid_node_decl(id: &str) -> String {
    mermaid_id(id)
}

/// Map a requirement status string to a DOT fill colour.
fn status_color(status: Option<&str>) -> Option<&'static str> {
    let s = status?.to_ascii_uppercase();
    if s == "COMPLETE" || s == "DONE" {
        Some("green")
    } else if s == "MISSING" {
        Some("yellow")
    } else if s.contains("BLOCK") {
        Some("red")
    } else {
        None
    }
}

/// Map a requirement status string to a Mermaid `classDef` name.
fn status_mermaid_class(status: Option<&str>) -> Option<&'static str> {
    let s = status?.to_ascii_uppercase();
    if s == "COMPLETE" || s == "DONE" {
        Some("complete")
    } else if s == "MISSING" {
        Some("missing")
    } else if s.contains("BLOCK") {
        Some("blocked")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-BUILD-001,BUILD,BINARY,Static binary,Runs on RHEL,tests/build.rs,test_binary,System Test,COMPLETE,CRITICAL,1,Rust musl,
REQ-TUI-001,TUI,LAYOUT,Chat layout,TUI renders,tests/tui.rs,test_layout,Unit Test,TODO,CRITICAL,1,ratatui,
REQ-AGENT-001,AGENT,LOOP,REA loop,Agent completes,tests/agent.rs,test_loop,Integration Test,COMPLETE,CRITICAL,1,Goose fork,REQ-LLM-001";

    // rtmx:req REQ-RTMX-001
    #[test]
    fn parse_csv_returns_all_requirements() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert_eq!(db.count(), 3);
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn get_requirement_by_id() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-BUILD-001").unwrap();
        assert_eq!(req.category, "BUILD");
        assert_eq!(req.status, "COMPLETE");
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn get_nonexistent_returns_none() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert!(db.get("REQ-FAKE-999").is_none());
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn filter_by_category() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let build = db.by_category("BUILD");
        assert_eq!(build.len(), 1);
        assert_eq!(build[0].req_id, "REQ-BUILD-001");
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn filter_by_status() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let complete = db.by_status("COMPLETE");
        assert_eq!(complete.len(), 2);
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn count_by_status() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert_eq!(db.count_by_status("COMPLETE"), 2);
        assert_eq!(db.count_by_status("TODO"), 1);
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn parse_dependencies() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-AGENT-001").unwrap();
        assert_eq!(req.dependencies, "REQ-LLM-001");
    }

    // rtmx:req REQ-AGENT-034
    #[test]
    fn dependency_ids_parses_pipe_delimited() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-AGENT-001").unwrap();
        assert_eq!(req.dependency_ids(), vec!["REQ-LLM-001"]);
    }

    // rtmx:req REQ-AGENT-034
    #[test]
    fn dependency_ids_empty_returns_empty_vec() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-BUILD-001").unwrap();
        assert!(req.dependency_ids().is_empty());
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn handles_quoted_fields_with_commas() {
        let csv = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-TEST-001,TEST,X,\"Requirement with, comma\",\"Target with, comma\",t.rs,test_fn,Unit Test,TODO,HIGH,1,\"Notes, here\",";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let req = db.get("REQ-TEST-001").unwrap();
        assert_eq!(req.requirement_text, "Requirement with, comma");
        assert_eq!(req.target_value, "Target with, comma");
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn empty_csv_returns_error() {
        let result = RequirementsDb::from_csv("");
        assert!(result.is_err());
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn loads_real_database() {
        let path = std::path::Path::new(".rtmx/database.csv");
        if path.exists() {
            let db = RequirementsDb::load(path).unwrap();
            assert!(
                db.count() > 100,
                "Real database should have 100+ requirements, got {}",
                db.count()
            );
            // Verify we can find a known requirement
            assert!(db.get("REQ-BUILD-001").is_some());
        }
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn update_status_changes_the_field() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_status("REQ-TUI-001", "IN_PROGRESS").unwrap();
        assert_eq!(db.get("REQ-TUI-001").unwrap().status, "IN_PROGRESS");
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn update_status_nonexistent_req_returns_error() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let result = db.update_status("REQ-FAKE-999", "DONE");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("REQ-FAKE-999"),
            "Error should mention the missing req_id"
        );
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn update_test_info_sets_both_fields() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_test_info("REQ-TUI-001", "tests/tui/new.rs", "test_new_layout")
            .unwrap();
        let req = db.get("REQ-TUI-001").unwrap();
        assert_eq!(req.test_module, "tests/tui/new.rs");
        assert_eq!(req.test_function, "test_new_layout");
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn set_completed_updates_status_and_date() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.set_completed("REQ-TUI-001").unwrap();
        let req = db.get("REQ-TUI-001").unwrap();
        assert_eq!(req.status, "COMPLETE");
        // Date should be today in YYYY-MM-DD format.
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        assert_eq!(req.completed_date, today);
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn save_csv_roundtrips_correctly() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_status("REQ-TUI-001", "IN_PROGRESS").unwrap();

        let dir = std::env::temp_dir().join("aegis_test_roundtrip");
        let path = dir.join("database.csv");
        db.save_csv(&path).unwrap();

        let db2 = RequirementsDb::load(&path).unwrap();
        assert_eq!(db2.count(), 3);
        assert_eq!(db2.get("REQ-TUI-001").unwrap().status, "IN_PROGRESS");
        assert_eq!(db2.get("REQ-BUILD-001").unwrap().status, "COMPLETE");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn save_preserves_all_columns() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let dir = std::env::temp_dir().join("aegis_test_columns");
        let path = dir.join("database.csv");
        db.save_csv(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Header should contain all 19 column names
        let header_line = content.lines().next().unwrap();
        for col in &[
            "req_id",
            "category",
            "subcategory",
            "requirement_text",
            "target_value",
            "test_module",
            "test_function",
            "validation_method",
            "status",
            "priority",
            "phase",
            "notes",
            "effort_weeks",
            "dependencies",
            "blocks",
            "assignee",
            "sprint",
            "started_date",
            "completed_date",
        ] {
            assert!(header_line.contains(col), "Header missing column: {col}");
        }
        // Data rows preserved
        assert!(content.contains("REQ-BUILD-001"));
        assert!(content.contains("REQ-AGENT-001"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn multiple_updates_accumulate() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_status("REQ-TUI-001", "IN_PROGRESS").unwrap();
        db.update_test_info("REQ-TUI-001", "tests/tui/v2.rs", "test_v2")
            .unwrap();
        db.update_status("REQ-TUI-001", "COMPLETE").unwrap();

        let req = db.get("REQ-TUI-001").unwrap();
        assert_eq!(req.status, "COMPLETE");
        assert_eq!(req.test_module, "tests/tui/v2.rs");
        assert_eq!(req.test_function, "test_v2");
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn save_creates_parent_directory_if_missing() {
        let dir = std::env::temp_dir()
            .join("aegis_test_mkdir")
            .join("nested")
            .join("deep");
        let path = dir.join("database.csv");

        // Ensure it does not exist
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("aegis_test_mkdir"));

        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.save_csv(&path).unwrap();

        assert!(path.exists(), "CSV file should have been created");
        let db2 = RequirementsDb::load(&path).unwrap();
        assert_eq!(db2.count(), 3);

        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("aegis_test_mkdir"));
    }

    // ------------------------------------------------------------------
    // DependencyGraph unit tests (REQ-RTMX-012 / REQ-RTMX-013)
    // ------------------------------------------------------------------

    /// Minimal CSV fixture whose dependencies column encodes a
    /// REQ-A -> REQ-B -> REQ-C chain (REQ-A depends on REQ-B, etc.).
    const CHAIN_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-A,CAT,X,a,t,,,,TODO,HIGH,1,,REQ-B
REQ-B,CAT,X,b,t,,,,TODO,HIGH,1,,REQ-C
REQ-C,CAT,X,c,t,,,,TODO,HIGH,1,,";

    /// CSV fixture with a 2-cycle (REQ-A <-> REQ-B).
    const CYCLE_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-A,CAT,X,a,t,,,,TODO,HIGH,1,,REQ-B
REQ-B,CAT,X,b,t,,,,TODO,HIGH,1,,REQ-A";

    // rtmx:req REQ-RTMX-012
    #[test]
    fn graph_topological_order_for_dag() {
        let db = RequirementsDb::from_csv(CHAIN_CSV).unwrap();
        let g = DependencyGraph::from_db(&db);

        let order = g.topological_order().expect("DAG must yield an order");
        // Dependencies must appear before dependents.
        let pos = |id: &str| order.iter().position(|s| s == id).unwrap();
        assert!(pos("REQ-C") < pos("REQ-B"));
        assert!(pos("REQ-B") < pos("REQ-A"));
        assert!(g.is_dag());
    }

    // rtmx:req REQ-RTMX-012
    #[test]
    fn graph_topological_order_err_when_cyclic() {
        let db = RequirementsDb::from_csv(CYCLE_CSV).unwrap();
        let g = DependencyGraph::from_db(&db);
        assert!(g.topological_order().is_err());
        assert!(!g.is_dag());
    }

    // rtmx:req REQ-RTMX-012
    #[test]
    fn graph_from_db_builds_expected_edges() {
        let db = RequirementsDb::from_csv(CHAIN_CSV).unwrap();
        let g = DependencyGraph::from_db(&db);
        assert!(g.dependencies("REQ-A").contains(&"REQ-B"));
        assert!(g.dependencies("REQ-B").contains(&"REQ-C"));
        assert_eq!(g.edges.len(), 3);
    }

    // rtmx:req REQ-RTMX-013
    #[test]
    fn to_dot_emits_quoted_edges_and_header() {
        let db = RequirementsDb::from_csv(CHAIN_CSV).unwrap();
        let g = DependencyGraph::from_db(&db);
        let dot = g.to_dot();
        assert!(dot.starts_with("digraph deps {"));
        assert!(dot.contains("\"REQ-A\" -> \"REQ-B\";"));
        assert!(dot.trim_end().ends_with('}'));
    }

    // rtmx:req REQ-RTMX-013
    #[test]
    fn to_mermaid_emits_graph_td_and_edges() {
        let db = RequirementsDb::from_csv(CHAIN_CSV).unwrap();
        let g = DependencyGraph::from_db(&db);
        let m = g.to_mermaid();
        assert!(m.starts_with("graph TD"));
        assert!(m.contains("REQ-A --> REQ-B"));
    }

    // rtmx:req REQ-RTMX-013
    #[test]
    fn status_color_maps_known_statuses() {
        assert_eq!(status_color(Some("COMPLETE")), Some("green"));
        assert_eq!(status_color(Some("complete")), Some("green"));
        assert_eq!(status_color(Some("MISSING")), Some("yellow"));
        assert_eq!(status_color(Some("BLOCKED")), Some("red"));
        assert_eq!(status_color(Some("TODO")), None);
        assert_eq!(status_color(None), None);
    }
}
