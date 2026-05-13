//! Dependency graph visualization and cycle detection for RTMX requirements.
//!
//! Implements REQ-RTMX-006: build a directed graph from the RTM database,
//! detect cycles via DFS, perform topological sort, and emit DOT format.
//!
//! This module provides a simple `DepGraph` facade using a
//! `HashMap<String, Vec<String>>` adjacency list.  Heavy lifting for
//! production graph operations (Tarjan SCC, Kahn's topo-sort, Mermaid
//! rendering) lives in [`crate::rtmx::DependencyGraph`].

use std::collections::HashMap;

use crate::rtmx::RequirementsDb;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Error returned when a topological sort is impossible because the graph
/// contains at least one cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleError {
    /// Requirement IDs that participate in at least one cycle.
    pub members: Vec<String>,
}

impl std::fmt::Display for CycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "dependency cycle detected among: {}",
            self.members.join(", ")
        )
    }
}

impl std::error::Error for CycleError {}

/// A directed dependency graph built from a `RequirementsDb`.
///
/// Edges are stored as an adjacency list: `node -> Vec<dependency>`.
/// A directed edge `A -> B` means "A depends on B".
#[derive(Debug, Clone)]
pub struct DepGraph {
    /// Adjacency list: req_id -> list of req_ids that this req depends on.
    pub adj: HashMap<String, Vec<String>>,
}

impl DepGraph {
    /// Build the graph from every requirement in `db`.
    ///
    /// Every requirement gets a node even when it has no dependencies.
    /// Dependencies listed in the CSV that do not correspond to a known
    /// requirement ID are still added as edges (dangling edges surface via
    /// `detect_cycles` / `topological_sort` as orphan nodes).
    ///
    /// # Examples
    ///
    /// ```
    /// // rtmx:req REQ-RTMX-006
    /// use aegis_domain::dep_graph::DepGraph;
    /// use aegis_domain::rtmx::RequirementsDb;
    ///
    /// let csv = "req_id,category,subcategory,requirement_text,target_value,\
    ///     test_module,test_function,validation_method,status,priority,phase,notes,dependencies\n\
    ///     REQ-A,C,S,t,v,m,f,Unit Test,TODO,HIGH,1,,REQ-B\n\
    ///     REQ-B,C,S,t,v,m,f,Unit Test,TODO,HIGH,1,,";
    /// let db = RequirementsDb::from_csv(csv).unwrap();
    /// let g = DepGraph::from_db(&db);
    /// assert!(g.adj["REQ-A"].contains(&"REQ-B".to_string()));
    /// assert!(g.adj["REQ-B"].is_empty());
    /// ```
    pub fn from_db(db: &RequirementsDb) -> Self {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for req in db.all() {
            adj.entry(req.req_id.clone()).or_default();
            for dep_id in req.dependency_ids() {
                if !dep_id.is_empty() {
                    adj.entry(req.req_id.clone())
                        .or_default()
                        .push(dep_id.to_string());
                }
            }
        }

        Self { adj }
    }

    /// Detect cycles using Tarjan's strongly-connected-components algorithm.
    ///
    /// Returns a `Vec` of cycles; each cycle is itself a `Vec<String>` of
    /// requirement IDs whose edges form a strongly-connected component with
    /// more than one member, or a singleton with a self-loop.
    ///
    /// Returns an empty `Vec` when the graph is acyclic.
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        self.to_dependency_graph().cycles()
    }

    /// Topological sort (Kahn's algorithm).
    ///
    /// Returns `Ok(Vec<String>)` with requirement IDs listed in dependency
    /// order (dependencies before dependents) when the graph is acyclic.
    /// Returns `Err(CycleError)` listing the nodes involved in cycles when
    /// at least one cycle exists.
    pub fn topological_sort(&self) -> Result<Vec<String>, CycleError> {
        self.to_dependency_graph()
            .topological_order()
            .map_err(|members| CycleError { members })
    }

    /// Render the graph as a Graphviz DOT document.
    ///
    /// Nodes are sorted for deterministic output.  Isolated nodes (no edges)
    /// are still emitted so the DOT document contains every requirement.
    ///
    /// # Examples
    ///
    /// ```
    /// // rtmx:req REQ-RTMX-006
    /// use aegis_domain::dep_graph::DepGraph;
    /// use aegis_domain::rtmx::RequirementsDb;
    ///
    /// let csv = "req_id,category,subcategory,requirement_text,target_value,\
    ///     test_module,test_function,validation_method,status,priority,phase,notes,dependencies\n\
    ///     REQ-A,C,S,t,v,m,f,Unit Test,TODO,HIGH,1,,REQ-B\n\
    ///     REQ-B,C,S,t,v,m,f,Unit Test,TODO,HIGH,1,,";
    /// let db = RequirementsDb::from_csv(csv).unwrap();
    /// let dot = DepGraph::from_db(&db).to_dot();
    /// assert!(dot.starts_with("digraph deps {"));
    /// assert!(dot.contains("\"REQ-A\" -> \"REQ-B\";"));
    /// ```
    pub fn to_dot(&self) -> String {
        self.to_dependency_graph().to_dot()
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Convert this `DepGraph` into a `DependencyGraph` for richer operations.
    fn to_dependency_graph(&self) -> crate::rtmx::DependencyGraph {
        use std::collections::HashSet;

        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        let mut reverse_edges: HashMap<String, HashSet<String>> = HashMap::new();

        for (from, deps) in &self.adj {
            edges.entry(from.clone()).or_default();
            reverse_edges.entry(from.clone()).or_default();
            for dep in deps {
                edges.entry(from.clone()).or_default().insert(dep.clone());
                reverse_edges
                    .entry(dep.clone())
                    .or_default()
                    .insert(from.clone());
            }
        }

        crate::rtmx::DependencyGraph {
            edges,
            reverse_edges,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtmx::RequirementsDb;

    /// Minimal CSV with a two-node dependency: REQ-A depends on REQ-B.
    const TWO_NODE_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,\
validation_method,status,priority,phase,notes,dependencies
REQ-A,CAT,X,a,t,m,f,Unit Test,TODO,HIGH,1,,REQ-B
REQ-B,CAT,X,b,t,m,f,Unit Test,TODO,HIGH,1,,";

    /// Chain CSV: REQ-C <- REQ-B <- REQ-A (A depends on B, B depends on C).
    const CHAIN_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,\
validation_method,status,priority,phase,notes,dependencies
REQ-A,CAT,X,a,t,m,f,Unit Test,TODO,HIGH,1,,REQ-B
REQ-B,CAT,X,b,t,m,f,Unit Test,TODO,HIGH,1,,REQ-C
REQ-C,CAT,X,c,t,m,f,Unit Test,TODO,HIGH,1,,";

    /// Cycle CSV: REQ-X -> REQ-Y -> REQ-X.
    const CYCLE_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,\
validation_method,status,priority,phase,notes,dependencies
REQ-X,CAT,X,x,t,m,f,Unit Test,TODO,HIGH,1,,REQ-Y
REQ-Y,CAT,X,y,t,m,f,Unit Test,TODO,HIGH,1,,REQ-X";

    // rtmx:req REQ-RTMX-006
    #[test]
    fn test_dep_graph_from_db_builds_edges() {
        let db = RequirementsDb::from_csv(TWO_NODE_CSV).unwrap();
        let g = DepGraph::from_db(&db);

        // Both nodes are present.
        assert!(g.adj.contains_key("REQ-A"), "REQ-A must be a node");
        assert!(g.adj.contains_key("REQ-B"), "REQ-B must be a node");

        // REQ-A has exactly one dependency: REQ-B.
        let a_deps = &g.adj["REQ-A"];
        assert_eq!(a_deps.len(), 1);
        assert!(a_deps.contains(&"REQ-B".to_string()));

        // REQ-B has no dependencies.
        assert!(g.adj["REQ-B"].is_empty());
    }

    // rtmx:req REQ-RTMX-006
    #[test]
    fn test_dep_graph_detects_cycle() {
        let db = RequirementsDb::from_csv(CYCLE_CSV).unwrap();
        let g = DepGraph::from_db(&db);

        let cycles = g.detect_cycles();
        assert!(
            !cycles.is_empty(),
            "Graph with REQ-X <-> REQ-Y cycle must report cycles"
        );

        // The cycle must involve both nodes.
        let all_members: Vec<String> = cycles.into_iter().flatten().collect();
        assert!(
            all_members.contains(&"REQ-X".to_string()),
            "REQ-X must appear in a cycle"
        );
        assert!(
            all_members.contains(&"REQ-Y".to_string()),
            "REQ-Y must appear in a cycle"
        );
    }

    // rtmx:req REQ-RTMX-006
    #[test]
    fn test_dep_graph_topological_sort() {
        let db = RequirementsDb::from_csv(CHAIN_CSV).unwrap();
        let g = DepGraph::from_db(&db);

        let order = g
            .topological_sort()
            .expect("chain is acyclic -- sort must succeed");
        assert_eq!(order.len(), 3);

        let pos = |id: &str| order.iter().position(|s| s == id).unwrap();
        // Dependencies must appear before their dependents.
        assert!(pos("REQ-C") < pos("REQ-B"), "REQ-C must precede REQ-B");
        assert!(pos("REQ-B") < pos("REQ-A"), "REQ-B must precede REQ-A");
    }

    // rtmx:req REQ-RTMX-006
    #[test]
    fn test_dep_graph_topological_sort_returns_err_on_cycle() {
        let db = RequirementsDb::from_csv(CYCLE_CSV).unwrap();
        let g = DepGraph::from_db(&db);

        let result = g.topological_sort();
        assert!(result.is_err(), "Cyclic graph must yield CycleError");

        let err = result.unwrap_err();
        assert!(
            err.members.contains(&"REQ-X".to_string())
                || err.members.contains(&"REQ-Y".to_string()),
            "CycleError members must include cycle participants"
        );
    }

    // rtmx:req REQ-RTMX-006
    #[test]
    fn test_dep_graph_to_dot_format() {
        let db = RequirementsDb::from_csv(TWO_NODE_CSV).unwrap();
        let g = DepGraph::from_db(&db);
        let dot = g.to_dot();

        // Must open with the digraph header.
        assert!(
            dot.starts_with("digraph deps {"),
            "DOT must start with digraph header"
        );
        // Must close with a brace.
        assert!(
            dot.trim_end().ends_with('}'),
            "DOT must end with closing brace"
        );
        // Must contain the dependency edge.
        assert!(
            dot.contains("\"REQ-A\" -> \"REQ-B\";"),
            "DOT must contain REQ-A -> REQ-B edge"
        );
        // REQ-B must appear as a node declaration.
        assert!(dot.contains("\"REQ-B\""), "DOT must declare REQ-B node");
    }

    // rtmx:req REQ-RTMX-006
    #[test]
    fn test_dep_graph_no_cycles_in_real_db() {
        let path = std::path::Path::new(".rtmx/database.csv");
        if !path.exists() {
            // Skip when the database is not present (e.g., in a stripped CI image).
            return;
        }
        let db = RequirementsDb::load(path).expect("real database must parse");
        let g = DepGraph::from_db(&db);
        let cycles = g.detect_cycles();
        assert!(
            cycles.is_empty(),
            "real database must have no dependency cycles; found: {:?}",
            cycles
        );
    }
}
