//! Infrastructure preview (dry-run) via plugin `preview` subcommand.
//!
//! Extracts a structured [`PreviewResult`] from the plugin's result event,
//! providing resource change counts and a human-readable summary suitable
//! for HITL approval before `up`.

use crate::events::ResultEvent;
use crate::host::PluginOutput;
use aegis_domain::error::DomainError;

/// Structured result of a plugin `preview` subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewResult {
    /// Number of resources to be created.
    pub creates: usize,
    /// Number of resources to be modified in-place.
    pub modifies: usize,
    /// Number of resources to be destroyed.
    pub destroys: usize,
    /// Human-readable summary from the plugin (or generated).
    pub summary: String,
    /// Raw outputs map from the plugin result event.
    pub raw_outputs: std::collections::HashMap<String, String>,
}

/// Parse a [`PreviewResult`] from a [`PluginOutput`].
///
/// Expects the plugin to have emitted a successful result event with
/// outputs containing `creates`, `modifies`, and `destroys` as string
/// counts. Missing count fields default to 0.
pub fn parse_preview_result(output: &PluginOutput) -> Result<PreviewResult, DomainError> {
    let result = output
        .result
        .as_ref()
        .ok_or_else(|| DomainError::ProviderError {
            message: "Plugin preview produced no result event".to_string(),
        })?;

    if !result.success {
        let msg = result.error.as_deref().unwrap_or("unknown error");
        return Err(DomainError::ProviderError {
            message: format!("Plugin preview failed: {msg}"),
        });
    }

    extract_from_result(result)
}

/// Extract preview counts and summary from a [`ResultEvent`].
fn extract_from_result(result: &ResultEvent) -> Result<PreviewResult, DomainError> {
    let outputs = result.outputs.clone().unwrap_or_default();

    let creates = parse_count(&outputs, "creates");
    let modifies = parse_count(&outputs, "modifies");
    let destroys = parse_count(&outputs, "destroys");

    let summary = result
        .summary
        .clone()
        .unwrap_or_else(|| default_summary(creates, modifies, destroys));

    Ok(PreviewResult {
        creates,
        modifies,
        destroys,
        summary,
        raw_outputs: outputs,
    })
}

/// Parse a count field from the outputs map, defaulting to 0.
fn parse_count(outputs: &std::collections::HashMap<String, String>, key: &str) -> usize {
    outputs
        .get(key)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
}

/// Generate a default summary when the plugin does not provide one.
fn default_summary(creates: usize, modifies: usize, destroys: usize) -> String {
    let total = creates + modifies + destroys;
    if total == 0 {
        "No changes detected.".to_string()
    } else {
        format!(
            "{total} resource(s): {creates} to create, \
             {modifies} to modify, {destroys} to destroy"
        )
    }
}

/// Format a [`PreviewResult`] for human-readable display.
pub fn format_preview(result: &PreviewResult) -> String {
    let mut lines = Vec::new();
    lines.push("Infrastructure Preview".to_string());
    lines.push("---------------------".to_string());
    lines.push(format!("  Create:  {}", result.creates));
    lines.push(format!("  Modify:  {}", result.modifies));
    lines.push(format!("  Destroy: {}", result.destroys));
    lines.push(String::new());
    lines.push(result.summary.clone());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ResultEvent;
    use crate::host::PluginOutput;
    use std::collections::HashMap;

    fn make_output(result: Option<ResultEvent>) -> PluginOutput {
        PluginOutput {
            events: Vec::new(),
            result,
            stderr: String::new(),
            exit_code: 0,
        }
    }

    // @req REQ-INFRA-012
    #[test]
    fn parse_successful_preview_result() {
        let mut outputs = HashMap::new();
        outputs.insert("creates".to_string(), "3".to_string());
        outputs.insert("modifies".to_string(), "1".to_string());
        outputs.insert("destroys".to_string(), "0".to_string());

        let result_event = ResultEvent {
            success: true,
            outputs: Some(outputs.clone()),
            error: None,
            summary: Some("3 new, 1 updated".to_string()),
        };

        let output = make_output(Some(result_event));
        let preview = parse_preview_result(&output).unwrap();

        assert_eq!(preview.creates, 3);
        assert_eq!(preview.modifies, 1);
        assert_eq!(preview.destroys, 0);
        assert_eq!(preview.summary, "3 new, 1 updated");
        assert_eq!(preview.raw_outputs, outputs);
    }

    // @req REQ-INFRA-012
    #[test]
    fn parse_preview_with_zero_changes() {
        let result_event = ResultEvent {
            success: true,
            outputs: Some(HashMap::new()),
            error: None,
            summary: None,
        };

        let output = make_output(Some(result_event));
        let preview = parse_preview_result(&output).unwrap();

        assert_eq!(preview.creates, 0);
        assert_eq!(preview.modifies, 0);
        assert_eq!(preview.destroys, 0);
        assert_eq!(preview.summary, "No changes detected.");
    }

    // @req REQ-INFRA-012
    #[test]
    fn parse_preview_missing_result_event_returns_error() {
        let output = make_output(None);
        let err = parse_preview_result(&output).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no result event"),
            "Should mention missing result: {msg}"
        );
    }

    // @req REQ-INFRA-012
    #[test]
    fn parse_preview_failed_result_returns_error() {
        let result_event = ResultEvent {
            success: false,
            outputs: None,
            error: Some("Quota exceeded".to_string()),
            summary: None,
        };

        let output = make_output(Some(result_event));
        let err = parse_preview_result(&output).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Quota exceeded"),
            "Should propagate error: {msg}"
        );
    }

    // @req REQ-INFRA-012
    #[test]
    fn parse_preview_defaults_missing_counts_to_zero() {
        let mut outputs = HashMap::new();
        outputs.insert("creates".to_string(), "5".to_string());
        // modifies and destroys are missing

        let result_event = ResultEvent {
            success: true,
            outputs: Some(outputs),
            error: None,
            summary: None,
        };

        let output = make_output(Some(result_event));
        let preview = parse_preview_result(&output).unwrap();

        assert_eq!(preview.creates, 5);
        assert_eq!(preview.modifies, 0);
        assert_eq!(preview.destroys, 0);
        assert!(preview.summary.contains("5 resource(s)"));
    }

    // @req REQ-INFRA-012
    #[test]
    fn parse_preview_generates_default_summary() {
        let mut outputs = HashMap::new();
        outputs.insert("creates".to_string(), "2".to_string());
        outputs.insert("modifies".to_string(), "1".to_string());
        outputs.insert("destroys".to_string(), "1".to_string());

        let result_event = ResultEvent {
            success: true,
            outputs: Some(outputs),
            error: None,
            summary: None,
        };

        let output = make_output(Some(result_event));
        let preview = parse_preview_result(&output).unwrap();

        assert_eq!(
            preview.summary,
            "4 resource(s): 2 to create, 1 to modify, 1 to destroy"
        );
    }

    // @req REQ-INFRA-012
    #[test]
    fn format_preview_output() {
        let preview = PreviewResult {
            creates: 3,
            modifies: 1,
            destroys: 0,
            summary: "3 new, 1 updated".to_string(),
            raw_outputs: HashMap::new(),
        };

        let formatted = format_preview(&preview);
        assert!(formatted.contains("Infrastructure Preview"));
        assert!(formatted.contains("Create:  3"));
        assert!(formatted.contains("Modify:  1"));
        assert!(formatted.contains("Destroy: 0"));
        assert!(formatted.contains("3 new, 1 updated"));
    }

    // @req REQ-INFRA-012
    #[test]
    fn parse_preview_with_no_outputs_field() {
        let result_event = ResultEvent {
            success: true,
            outputs: None,
            error: None,
            summary: None,
        };

        let output = make_output(Some(result_event));
        let preview = parse_preview_result(&output).unwrap();

        assert_eq!(preview.creates, 0);
        assert_eq!(preview.modifies, 0);
        assert_eq!(preview.destroys, 0);
        assert_eq!(preview.summary, "No changes detected.");
    }
}
