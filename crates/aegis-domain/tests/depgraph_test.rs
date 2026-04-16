//! Integration tests for REQ-RTMX-011: Dependency graph as DAG.

use aegis_domain::rtmx::{DependencyGraph, RequirementsDb};

const DAG_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-A-001,A,X,Requirement A,,,,Unit Test,TODO,HIGH,1,,
REQ-B-001,B,X,Requirement B,,,,Unit Test,TODO,HIGH,1,,REQ-A-001
REQ-C-001,C,X,Requirement C,,,,Unit Test,TODO,HIGH,1,,REQ-A-001|REQ-B-001
REQ-D-001,D,X,Requirement D,,,,Unit Test,TODO,HIGH,1,,";

// rtmx:req REQ-RTMX-011
#[test]
fn test_dag_construction() {
    let db = RequirementsDb::from_csv(DAG_CSV).unwrap();
    let graph = DependencyGraph::from_db(&db);

    // REQ-A-001 has no dependencies.
    assert!(graph.dependencies("REQ-A-001").is_empty());

    // REQ-B-001 depends on REQ-A-001.
    let b_deps = graph.dependencies("REQ-B-001");
    assert_eq!(b_deps.len(), 1);
    assert!(b_deps.contains(&"REQ-A-001"));

    // REQ-C-001 depends on both A and B.
    let c_deps = graph.dependencies("REQ-C-001");
    assert_eq!(c_deps.len(), 2);
    assert!(c_deps.contains(&"REQ-A-001"));
    assert!(c_deps.contains(&"REQ-B-001"));

    // REQ-A-001 is depended upon by B and C.
    let a_dependents = graph.dependents("REQ-A-001");
    assert!(a_dependents.contains(&"REQ-B-001"));
    assert!(a_dependents.contains(&"REQ-C-001"));

    // REQ-D-001 has no dependencies and no dependents.
    assert!(graph.dependencies("REQ-D-001").is_empty());
    assert!(graph.dependents("REQ-D-001").is_empty());
}

// rtmx:req REQ-RTMX-011
#[test]
fn test_topological_order() {
    let db = RequirementsDb::from_csv(DAG_CSV).unwrap();
    let graph = DependencyGraph::from_db(&db);

    let order = graph.topological_order().expect("should be a valid DAG");
    assert_eq!(order.len(), 4);

    // A must come before B, and both A and B must come before C.
    let pos_a = order.iter().position(|x| x == "REQ-A-001").unwrap();
    let pos_b = order.iter().position(|x| x == "REQ-B-001").unwrap();
    let pos_c = order.iter().position(|x| x == "REQ-C-001").unwrap();

    assert!(pos_a < pos_b, "A must come before B");
    assert!(pos_a < pos_c, "A must come before C");
    assert!(pos_b < pos_c, "B must come before C");
}

// rtmx:req REQ-RTMX-011
#[test]
fn test_cycle_detection() {
    let cycle_csv = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-X-001,X,X,Req X,,,,Unit Test,TODO,HIGH,1,,REQ-Y-001
REQ-Y-001,Y,X,Req Y,,,,Unit Test,TODO,HIGH,1,,REQ-Z-001
REQ-Z-001,Z,X,Req Z,,,,Unit Test,TODO,HIGH,1,,REQ-X-001";

    let db = RequirementsDb::from_csv(cycle_csv).unwrap();
    let graph = DependencyGraph::from_db(&db);

    assert!(!graph.is_dag(), "Graph with a cycle should not be a DAG");

    let err = graph.topological_order().unwrap_err();
    assert_eq!(err.len(), 3, "All three nodes are in the cycle");
}

// rtmx:req REQ-RTMX-011
#[test]
fn test_is_dag_returns_true_for_valid_dag() {
    let db = RequirementsDb::from_csv(DAG_CSV).unwrap();
    let graph = DependencyGraph::from_db(&db);
    assert!(graph.is_dag());
}
