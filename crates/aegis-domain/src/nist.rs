//! NIST 800-171 control mapping for RTMX requirements.
//!
//! Maps aegis requirement categories to NIST 800-171 Rev 2 control families,
//! enabling traceability from implementation requirements to compliance controls.

use crate::rtmx::Requirement;

/// A mapping from an aegis requirement to a NIST 800-171 control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NistMapping {
    /// NIST 800-171 control identifier, e.g. "3.1.1".
    pub control_id: &'static str,
    /// NIST 800-171 control family name, e.g. "Access Control".
    pub control_family: &'static str,
    /// Brief description of the control.
    pub description: &'static str,
}

/// Map a requirement to its applicable NIST 800-171 controls based on category
/// and subcategory.
pub fn map_requirement_to_nist(req: &Requirement) -> Vec<NistMapping> {
    let mut mappings = Vec::new();

    match req.category.as_str() {
        "AUDIT" => {
            mappings.push(NistMapping {
                control_id: "3.3.1",
                control_family: "Audit and Accountability",
                description: "Create and retain system audit logs and records",
            });
            mappings.push(NistMapping {
                control_id: "3.3.2",
                control_family: "Audit and Accountability",
                description: "Ensure actions can be uniquely traced to individual users",
            });
        }
        "SECURITY" => {
            mappings.push(NistMapping {
                control_id: "3.1.1",
                control_family: "Access Control",
                description: "Limit system access to authorized users",
            });
            mappings.push(NistMapping {
                control_id: "3.13.1",
                control_family: "System and Communications Protection",
                description: "Monitor, control, and protect communications at boundaries",
            });
        }
        "HITL" => {
            mappings.push(NistMapping {
                control_id: "3.1.7",
                control_family: "Access Control",
                description: "Prevent non-privileged users from executing privileged functions",
            });
        }
        "LLM" => {
            mappings.push(NistMapping {
                control_id: "3.13.1",
                control_family: "System and Communications Protection",
                description: "Monitor, control, and protect communications at boundaries",
            });
            mappings.push(NistMapping {
                control_id: "3.13.8",
                control_family: "System and Communications Protection",
                description: "Implement cryptographic mechanisms to prevent unauthorized disclosure",
            });
        }
        "BUILD" => {
            mappings.push(NistMapping {
                control_id: "3.4.1",
                control_family: "Configuration Management",
                description: "Establish and maintain baseline configurations",
            });
            mappings.push(NistMapping {
                control_id: "3.14.1",
                control_family: "System and Information Integrity",
                description: "Identify, report, and correct system flaws in a timely manner",
            });
        }
        "TUI" => {
            mappings.push(NistMapping {
                control_id: "3.1.1",
                control_family: "Access Control",
                description: "Limit system access to authorized users",
            });
        }
        "AGENT" => {
            mappings.push(NistMapping {
                control_id: "3.1.2",
                control_family: "Access Control",
                description: "Limit system access to authorized transactions and functions",
            });
            mappings.push(NistMapping {
                control_id: "3.3.1",
                control_family: "Audit and Accountability",
                description: "Create and retain system audit logs and records",
            });
        }
        "INFRA" => {
            mappings.push(NistMapping {
                control_id: "3.4.2",
                control_family: "Configuration Management",
                description: "Establish and enforce security configuration settings",
            });
            mappings.push(NistMapping {
                control_id: "3.13.1",
                control_family: "System and Communications Protection",
                description: "Monitor, control, and protect communications at boundaries",
            });
        }
        "ONBOARD" => {
            mappings.push(NistMapping {
                control_id: "3.4.1",
                control_family: "Configuration Management",
                description: "Establish and maintain baseline configurations",
            });
            mappings.push(NistMapping {
                control_id: "3.5.1",
                control_family: "Identification and Authentication",
                description: "Identify system users and authenticate identities",
            });
        }
        "RTMX" => {
            mappings.push(NistMapping {
                control_id: "3.12.1",
                control_family: "Security Assessment",
                description: "Periodically assess security controls for effectiveness",
            });
            mappings.push(NistMapping {
                control_id: "3.12.3",
                control_family: "Security Assessment",
                description: "Monitor security controls on an ongoing basis",
            });
        }
        "TEST" => {
            mappings.push(NistMapping {
                control_id: "3.14.1",
                control_family: "System and Information Integrity",
                description: "Identify, report, and correct system flaws in a timely manner",
            });
            mappings.push(NistMapping {
                control_id: "3.12.1",
                control_family: "Security Assessment",
                description: "Periodically assess security controls for effectiveness",
            });
        }
        _ => {
            // Unknown categories still get a baseline mapping.
            mappings.push(NistMapping {
                control_id: "3.14.1",
                control_family: "System and Information Integrity",
                description: "Identify, report, and correct system flaws in a timely manner",
            });
        }
    }

    mappings
}

/// All requirement categories known to the system.
pub const KNOWN_CATEGORIES: &[&str] = &[
    "AUDIT", "SECURITY", "HITL", "LLM", "BUILD", "TUI", "AGENT", "INFRA", "ONBOARD", "RTMX",
    "TEST",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtmx::Requirement;

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
    fn audit_maps_to_3_3() {
        let req = make_req("AUDIT");
        let mappings = map_requirement_to_nist(&req);
        assert!(!mappings.is_empty());
        assert!(
            mappings.iter().any(|m| m.control_id.starts_with("3.3")),
            "AUDIT should map to 3.3.x controls"
        );
    }

    // rtmx:req REQ-RTMX-005
    #[test]
    fn security_maps_to_access_control_and_comms_protection() {
        let req = make_req("SECURITY");
        let mappings = map_requirement_to_nist(&req);
        assert!(mappings.iter().any(|m| m.control_id.starts_with("3.1")));
        assert!(mappings.iter().any(|m| m.control_id.starts_with("3.13")));
    }

    // rtmx:req REQ-RTMX-005
    #[test]
    fn all_known_categories_have_mappings() {
        for cat in KNOWN_CATEGORIES {
            let req = make_req(cat);
            let mappings = map_requirement_to_nist(&req);
            assert!(
                !mappings.is_empty(),
                "Category {cat} should have at least one NIST mapping"
            );
        }
    }

    // rtmx:req REQ-RTMX-005
    #[test]
    fn unknown_category_gets_baseline() {
        let req = make_req("UNKNOWN");
        let mappings = map_requirement_to_nist(&req);
        assert!(
            !mappings.is_empty(),
            "Unknown categories should get a baseline mapping"
        );
    }
}
