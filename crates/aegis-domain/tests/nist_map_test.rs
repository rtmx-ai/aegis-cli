//! Integration tests for REQ-RTMX-005: NIST 800-171 control identifiers.

use aegis_domain::nist::{KNOWN_CATEGORIES, map_requirement_to_nist};
use aegis_domain::rtmx::Requirement;

fn make_req(category: &str) -> Requirement {
    Requirement {
        req_id: format!("REQ-{category}-001"),
        category: category.to_string(),
        subcategory: String::new(),
        requirement_text: String::new(),
        target_value: String::new(),
        test_module: String::new(),
        test_function: String::new(),
        validation_method: String::new(),
        status: "TODO".to_string(),
        priority: String::new(),
        phase: String::new(),
        notes: String::new(),
        effort_weeks: String::new(),
        dependencies: String::new(),
        blocks: String::new(),
        assignee: String::new(),
        sprint: String::new(),
        started_date: String::new(),
        completed_date: String::new(),
    }
}

// rtmx:req REQ-RTMX-005
#[test]
fn test_nist_mapping() {
    let req = make_req("AUDIT");
    let mappings = map_requirement_to_nist(&req);
    assert!(!mappings.is_empty(), "AUDIT should have NIST mappings");
    assert!(
        mappings.iter().any(|m| m.control_id.starts_with("3.3")),
        "AUDIT requirements should map to 3.3.x (Audit and Accountability)"
    );
    assert!(
        mappings
            .iter()
            .any(|m| m.control_family == "Audit and Accountability"),
        "Control family should be Audit and Accountability"
    );
}

// rtmx:req REQ-RTMX-005
#[test]
fn test_all_categories_have_mappings() {
    for cat in KNOWN_CATEGORIES {
        let req = make_req(cat);
        let mappings = map_requirement_to_nist(&req);
        assert!(
            !mappings.is_empty(),
            "Category {cat} must have at least one NIST 800-171 control mapping"
        );
        for m in &mappings {
            assert!(!m.control_id.is_empty(), "Control ID must not be empty");
            assert!(
                !m.control_family.is_empty(),
                "Control family must not be empty"
            );
            assert!(!m.description.is_empty(), "Description must not be empty");
        }
    }
}

// rtmx:req REQ-RTMX-005
#[test]
fn test_security_maps_to_access_and_comms() {
    let req = make_req("SECURITY");
    let mappings = map_requirement_to_nist(&req);
    let families: Vec<&str> = mappings.iter().map(|m| m.control_family).collect();
    assert!(families.contains(&"Access Control"));
    assert!(families.contains(&"System and Communications Protection"));
}

// rtmx:req REQ-RTMX-005
#[test]
fn test_hitl_maps_to_access_control() {
    let req = make_req("HITL");
    let mappings = map_requirement_to_nist(&req);
    assert!(
        mappings
            .iter()
            .any(|m| m.control_family == "Access Control"),
        "HITL should map to Access Control"
    );
}

// rtmx:req REQ-RTMX-005
#[test]
fn test_llm_maps_to_comms_protection() {
    let req = make_req("LLM");
    let mappings = map_requirement_to_nist(&req);
    assert!(
        mappings
            .iter()
            .any(|m| m.control_family == "System and Communications Protection"),
        "LLM should map to System and Communications Protection"
    );
}
