//! Plugin output persistence for REQ-INFRA-008.
//!
//! After a successful plugin `up` command, the result outputs (endpoint URLs,
//! resource IDs, etc.) are merged into `~/.aegis/config.yaml` under the
//! `infra` section. New outputs merge with existing ones -- keys from the
//! new run overwrite colliding keys, but unrelated plugin outputs are
//! preserved.

use crate::host::PluginOutput;
use std::collections::HashMap;

/// Merge new outputs into an existing map.
///
/// Keys present in `new` overwrite the same key in `existing`.
/// Keys only in `existing` are preserved.
pub fn merge_outputs(
    existing: &HashMap<String, String>,
    new: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = existing.clone();
    for (k, v) in new {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Extract the outputs map from a successful plugin result event.
///
/// Returns an empty map if:
/// - There is no result event
/// - The result was not successful
/// - The result has no outputs
pub fn extract_outputs(output: &PluginOutput) -> HashMap<String, String> {
    match &output.result {
        Some(r) if r.success => r.outputs.clone().unwrap_or_default(),
        _ => HashMap::new(),
    }
}

/// Format outputs as a human-readable `key=value` list, one per line.
///
/// Keys are sorted alphabetically for deterministic output.
/// Returns an empty string if the map is empty.
pub fn format_outputs(outputs: &HashMap<String, String>) -> String {
    if outputs.is_empty() {
        return String::new();
    }
    let mut keys: Vec<&String> = outputs.keys().collect();
    keys.sort();
    keys.iter()
        .map(|k| format!("{}={}", k, outputs[*k]))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ResultEvent;

    // rtmx:req REQ-INFRA-008
    #[test]
    fn merge_outputs_combines_disjoint_maps() {
        let existing = HashMap::from([("vpc_id".to_string(), "vpc-abc".to_string())]);
        let new = HashMap::from([("endpoint".to_string(), "https://vertex.example".to_string())]);
        let merged = merge_outputs(&existing, &new);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged["vpc_id"], "vpc-abc");
        assert_eq!(merged["endpoint"], "https://vertex.example");
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn merge_outputs_new_overwrites_collision() {
        let existing =
            HashMap::from([("endpoint".to_string(), "https://old.example".to_string())]);
        let new = HashMap::from([("endpoint".to_string(), "https://new.example".to_string())]);
        let merged = merge_outputs(&existing, &new);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged["endpoint"], "https://new.example");
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn merge_outputs_empty_new_preserves_existing() {
        let existing = HashMap::from([("vpc_id".to_string(), "vpc-abc".to_string())]);
        let merged = merge_outputs(&existing, &HashMap::new());
        assert_eq!(merged, existing);
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn merge_outputs_empty_existing_returns_new() {
        let new = HashMap::from([("endpoint".to_string(), "https://new.example".to_string())]);
        let merged = merge_outputs(&HashMap::new(), &new);
        assert_eq!(merged, new);
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn merge_outputs_both_empty() {
        let merged = merge_outputs(&HashMap::new(), &HashMap::new());
        assert!(merged.is_empty());
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn extract_outputs_from_successful_result() {
        let mut outputs = HashMap::new();
        outputs.insert(
            "vertex_endpoint".to_string(),
            "us-central1-aiplatform.googleapis.com".to_string(),
        );
        outputs.insert("vpc_name".to_string(), "aegis-vpc".to_string());

        let plugin_output = PluginOutput {
            events: vec![],
            result: Some(ResultEvent {
                success: true,
                outputs: Some(outputs.clone()),
                error: None,
                summary: None,
            }),
            stderr: String::new(),
            exit_code: 0,
        };

        let extracted = extract_outputs(&plugin_output);
        assert_eq!(extracted.len(), 2);
        assert_eq!(
            extracted["vertex_endpoint"],
            "us-central1-aiplatform.googleapis.com"
        );
        assert_eq!(extracted["vpc_name"], "aegis-vpc");
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn extract_outputs_returns_empty_on_failure() {
        let plugin_output = PluginOutput {
            events: vec![],
            result: Some(ResultEvent {
                success: false,
                outputs: Some(HashMap::from([("key".to_string(), "value".to_string())])),
                error: Some("Quota exceeded".to_string()),
                summary: None,
            }),
            stderr: String::new(),
            exit_code: 1,
        };

        let extracted = extract_outputs(&plugin_output);
        assert!(
            extracted.is_empty(),
            "Failed results should yield no outputs"
        );
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn extract_outputs_returns_empty_when_no_result() {
        let plugin_output = PluginOutput {
            events: vec![],
            result: None,
            stderr: String::new(),
            exit_code: 0,
        };

        let extracted = extract_outputs(&plugin_output);
        assert!(extracted.is_empty());
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn extract_outputs_returns_empty_when_outputs_is_none() {
        let plugin_output = PluginOutput {
            events: vec![],
            result: Some(ResultEvent {
                success: true,
                outputs: None,
                error: None,
                summary: Some("Done".to_string()),
            }),
            stderr: String::new(),
            exit_code: 0,
        };

        let extracted = extract_outputs(&plugin_output);
        assert!(extracted.is_empty());
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn format_outputs_sorted_key_value() {
        let outputs = HashMap::from([
            ("vpc_name".to_string(), "aegis-vpc".to_string()),
            ("endpoint".to_string(), "https://vertex.example".to_string()),
            ("project_id".to_string(), "aegis-il4-prod".to_string()),
        ]);
        let formatted = format_outputs(&outputs);
        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "endpoint=https://vertex.example");
        assert_eq!(lines[1], "project_id=aegis-il4-prod");
        assert_eq!(lines[2], "vpc_name=aegis-vpc");
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn format_outputs_empty_map_returns_empty_string() {
        let formatted = format_outputs(&HashMap::new());
        assert!(formatted.is_empty());
    }

    // rtmx:req REQ-INFRA-008
    #[test]
    fn format_outputs_single_entry() {
        let outputs = HashMap::from([("key".to_string(), "value".to_string())]);
        let formatted = format_outputs(&outputs);
        assert_eq!(formatted, "key=value");
    }
}
