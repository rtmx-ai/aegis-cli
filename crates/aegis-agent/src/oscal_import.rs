//! OSCAL JSON schema parser and RTM mapping layer.
//!
//! Parses NIST OSCAL JSON catalog format into `OscalControl` structs,
//! then maps them to `RtmImportRow` for import into the RTM database.

use aegis_domain::DomainError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A parsed OSCAL control from a catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscalControl {
    pub id: String,
    pub title: String,
    pub description: String,
    pub parameters: Vec<String>,
}

/// A row ready for import into the RTM database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtmImportRow {
    pub req_id: String,
    pub category: String,
    pub requirement_text: String,
    pub priority: String,
    pub status: String,
}

/// Parse NIST OSCAL JSON catalog format into a list of controls.
///
/// Walks the `catalog.groups` tree (including nested subgroups) and extracts
/// each control's id, title, statement prose, and parameter labels.
pub fn parse_oscal(json: &str) -> Result<Vec<OscalControl>, DomainError> {
    let root: Value = serde_json::from_str(json)
        .map_err(|e| DomainError::Other(format!("Invalid OSCAL JSON: {e}")))?;

    let groups = root
        .pointer("/catalog/groups")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut controls = Vec::new();
    for group in &groups {
        extract_controls_recursive(group, &mut controls);
    }
    Ok(controls)
}

/// Recursively extract controls from a group and its subgroups.
fn extract_controls_recursive(group: &Value, out: &mut Vec<OscalControl>) {
    if let Some(controls) = group.get("controls").and_then(|v| v.as_array()) {
        for ctrl in controls {
            if let Some(control) = parse_single_control(ctrl) {
                out.push(control);
            }
        }
    }
    if let Some(subgroups) = group.get("groups").and_then(|v| v.as_array()) {
        for subgroup in subgroups {
            extract_controls_recursive(subgroup, out);
        }
    }
}

/// Parse a single control JSON value into an `OscalControl`.
fn parse_single_control(ctrl: &Value) -> Option<OscalControl> {
    let id = ctrl.get("id")?.as_str()?.to_string();
    let title = ctrl
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let description = ctrl
        .get("parts")
        .and_then(|v| v.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p.get("name").and_then(|n| n.as_str()) == Some("statement"))
                .filter_map(|p| p.get("prose").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

    let parameters = ctrl
        .get("params")
        .and_then(|v| v.as_array())
        .map(|params| {
            params
                .iter()
                .filter_map(|p| p.get("label").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Some(OscalControl {
        id,
        title,
        description,
        parameters,
    })
}

/// Map a slice of `OscalControl` values to RTM import rows.
///
/// Generates `req_id` as `REQ-NIST-{control_id}` with the control id
/// uppercased and dots replaced by hyphens. Sets category to `COMPLIANCE`,
/// status to `MISSING`, and priority to `MEDIUM`.
pub fn oscal_to_rtm(controls: &[OscalControl]) -> Vec<RtmImportRow> {
    controls
        .iter()
        .map(|ctrl| {
            let normalized_id = ctrl.id.to_uppercase().replace('.', "-");
            RtmImportRow {
                req_id: format!("REQ-NIST-{normalized_id}"),
                category: "COMPLIANCE".to_string(),
                requirement_text: format!("{}: {}", ctrl.title, ctrl.description),
                priority: "MEDIUM".to_string(),
                status: "MISSING".to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_oscal_json() -> &'static str {
        r#"{
            "catalog": {
                "uuid": "test-uuid",
                "metadata": { "title": "Test Catalog", "version": "1.0" },
                "groups": [
                    {
                        "id": "ac",
                        "title": "Access Control",
                        "controls": [
                            {
                                "id": "ac-1",
                                "title": "Policy and Procedures",
                                "parts": [
                                    {
                                        "id": "ac-1_smt",
                                        "name": "statement",
                                        "prose": "The organization defines access control policy."
                                    }
                                ],
                                "params": [
                                    {
                                        "id": "ac-1_prm_1",
                                        "label": "organization-defined frequency"
                                    }
                                ]
                            },
                            {
                                "id": "ac-2",
                                "title": "Account Management",
                                "parts": [
                                    {
                                        "id": "ac-2_smt",
                                        "name": "statement",
                                        "prose": "Manage system accounts."
                                    }
                                ],
                                "params": []
                            }
                        ]
                    }
                ]
            }
        }"#
    }

    // rtmx:req REQ-RTMX-028
    #[test]
    fn test_oscal_parser_extracts_controls() {
        let controls = parse_oscal(sample_oscal_json()).unwrap();
        assert_eq!(controls.len(), 2);
        assert_eq!(controls[0].id, "ac-1");
        assert_eq!(controls[0].title, "Policy and Procedures");
        assert_eq!(
            controls[0].description,
            "The organization defines access control policy."
        );
        assert_eq!(
            controls[0].parameters,
            vec!["organization-defined frequency"]
        );
        assert_eq!(controls[1].id, "ac-2");
        assert_eq!(controls[1].title, "Account Management");
    }

    // rtmx:req REQ-RTMX-028
    #[test]
    fn test_oscal_parser_handles_nested_groups() {
        let json = r#"{
            "catalog": {
                "uuid": "test",
                "metadata": { "title": "Test", "version": "1.0" },
                "groups": [
                    {
                        "id": "ac",
                        "title": "Access Control",
                        "controls": [
                            { "id": "ac-1", "title": "Top Level" }
                        ],
                        "groups": [
                            {
                                "id": "ac.sub",
                                "title": "Sub Group",
                                "controls": [
                                    { "id": "ac-1.1", "title": "Nested Control" }
                                ]
                            }
                        ]
                    }
                ]
            }
        }"#;
        let controls = parse_oscal(json).unwrap();
        assert_eq!(controls.len(), 2);
        assert_eq!(controls[0].id, "ac-1");
        assert_eq!(controls[1].id, "ac-1.1");
        assert_eq!(controls[1].title, "Nested Control");
    }

    // rtmx:req REQ-RTMX-028
    #[test]
    fn test_oscal_parser_empty_catalog() {
        let json = r#"{
            "catalog": {
                "uuid": "test",
                "metadata": { "title": "Empty", "version": "1.0" },
                "groups": []
            }
        }"#;
        let controls = parse_oscal(json).unwrap();
        assert!(controls.is_empty());
    }

    // rtmx:req REQ-RTMX-028
    #[test]
    fn test_oscal_parser_missing_parts() {
        let json = r#"{
            "catalog": {
                "uuid": "test",
                "metadata": { "title": "Test", "version": "1.0" },
                "groups": [
                    {
                        "id": "sc",
                        "title": "System and Communications",
                        "controls": [
                            { "id": "sc-1", "title": "No Parts Control" }
                        ]
                    }
                ]
            }
        }"#;
        let controls = parse_oscal(json).unwrap();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].id, "sc-1");
        assert_eq!(controls[0].description, "");
        assert!(controls[0].parameters.is_empty());
    }

    // rtmx:req REQ-RTMX-029
    #[test]
    fn test_oscal_to_rtm_generates_req_ids() {
        let controls = vec![
            OscalControl {
                id: "ac-1".to_string(),
                title: "Policy".to_string(),
                description: "Desc".to_string(),
                parameters: vec![],
            },
            OscalControl {
                id: "ac-1.1".to_string(),
                title: "Sub Policy".to_string(),
                description: "Sub desc".to_string(),
                parameters: vec![],
            },
        ];
        let rows = oscal_to_rtm(&controls);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].req_id, "REQ-NIST-AC-1");
        assert_eq!(rows[1].req_id, "REQ-NIST-AC-1-1");
    }

    // rtmx:req REQ-RTMX-029
    #[test]
    fn test_oscal_to_rtm_sets_defaults() {
        let controls = vec![OscalControl {
            id: "cm-7".to_string(),
            title: "Least Functionality".to_string(),
            description: "Configure the system.".to_string(),
            parameters: vec![],
        }];
        let rows = oscal_to_rtm(&controls);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].category, "COMPLIANCE");
        assert_eq!(rows[0].status, "MISSING");
        assert_eq!(rows[0].priority, "MEDIUM");
        assert_eq!(
            rows[0].requirement_text,
            "Least Functionality: Configure the system."
        );
    }

    // rtmx:req REQ-RTMX-028
    #[test]
    fn test_oscal_parser_invalid_json() {
        let result = parse_oscal("not valid json {{{");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid OSCAL JSON"));
    }

    // rtmx:req REQ-RTMX-014
    // rtmx:req REQ-RTMX-010
    #[test]
    fn test_oscal_json_import_end_to_end() {
        // Verify full pipeline: parse OSCAL JSON -> map to RTM rows.
        let json = r#"{
            "catalog": {
                "groups": [{
                    "id": "ac",
                    "title": "Access Control",
                    "controls": [{
                        "id": "ac-2",
                        "title": "Account Management",
                        "parts": [{"prose": "Manage accounts."}],
                        "params": []
                    }]
                }]
            }
        }"#;
        let controls = parse_oscal(json).unwrap();
        assert!(!controls.is_empty(), "must parse at least one control");
        let rows = oscal_to_rtm(&controls);
        assert!(!rows.is_empty(), "must produce RTM rows");
        assert!(
            rows[0].req_id.starts_with("REQ-NIST-"),
            "OSCAL imports must use REQ-NIST- prefix"
        );
    }
}
