//! Plugin preview and confirmation before auto-provisioning.
//!
//! Before running a plugin's `up` command, shows a preview of what
//! will be provisioned. Runs the plugin's `preview` subcommand,
//! parses NDJSON output for resource descriptions, and formats them
//! for display.

use std::path::Path;

/// A preview of what a plugin will provision.
#[derive(Debug, Clone)]
pub struct PluginPreview {
    /// Name of the plugin producing this preview.
    pub plugin_name: String,
    /// Descriptions of resources to be created/modified.
    pub resources: Vec<String>,
    /// Whether the user must confirm before proceeding.
    pub requires_confirmation: bool,
}

/// Errors that can occur during preview.
#[derive(Debug)]
pub enum PreviewError {
    /// The plugin binary was not found or could not be executed.
    PluginNotFound(String),
    /// The plugin exited with an error.
    PluginFailed(String),
    /// Failed to parse the plugin's NDJSON output.
    ParseError(String),
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewError::PluginNotFound(msg) => {
                write!(f, "plugin not found: {msg}")
            }
            PreviewError::PluginFailed(msg) => {
                write!(f, "plugin preview failed: {msg}")
            }
            PreviewError::ParseError(msg) => {
                write!(f, "preview parse error: {msg}")
            }
        }
    }
}

impl std::error::Error for PreviewError {}

/// Parse NDJSON lines from a plugin preview into resource descriptions.
///
/// Looks for progress events (type=progress) and extracts resource
/// descriptions. Also collects diagnostic messages as context.
pub fn parse_preview_output(ndjson_lines: &str) -> Vec<String> {
    let mut resources = Vec::new();

    for line in ndjson_lines.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "progress" => {
                let resource = parsed
                    .get("resource")
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown");
                let operation = parsed
                    .get("operation")
                    .and_then(|o| o.as_str())
                    .unwrap_or("create");
                let name = parsed.get("name").and_then(|n| n.as_str());

                let desc = match name {
                    Some(n) => {
                        format!("{operation} {resource} ({n})")
                    }
                    None => format!("{operation} {resource}"),
                };
                resources.push(desc);
            }
            "diagnostic" => {
                if let Some(msg) = parsed.get("message").and_then(|m| m.as_str()) {
                    let severity = parsed
                        .get("severity")
                        .and_then(|s| s.as_str())
                        .unwrap_or("info");
                    resources.push(format!("[{severity}] {msg}"));
                }
            }
            _ => {}
        }
    }

    resources
}

/// Build a PluginPreview from raw NDJSON output.
pub fn build_preview(plugin_name: &str, ndjson_output: &str) -> PluginPreview {
    let resources = parse_preview_output(ndjson_output);
    PluginPreview {
        plugin_name: plugin_name.to_string(),
        resources,
        requires_confirmation: true,
    }
}

/// Get a preview from a plugin binary path.
///
/// This is the async entry point that spawns the plugin subprocess.
/// For unit tests, use [`build_preview`] with mock output instead.
pub async fn get_plugin_preview(plugin_path: &Path) -> Result<PluginPreview, PreviewError> {
    if !plugin_path.exists() {
        return Err(PreviewError::PluginNotFound(format!(
            "Plugin binary not found at {}",
            plugin_path.display()
        )));
    }

    let output = tokio::process::Command::new(plugin_path)
        .arg("preview")
        .output()
        .await
        .map_err(|e| {
            PreviewError::PluginFailed(format!("Failed to run {}: {e}", plugin_path.display()))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PreviewError::PluginFailed(format!(
            "Plugin exited with {}: {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let plugin_name = plugin_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    Ok(build_preview(plugin_name, &stdout))
}

/// Format a preview for terminal display.
pub fn format_preview_for_display(preview: &PluginPreview) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Plugin '{}' will perform the following actions:",
        preview.plugin_name
    ));
    lines.push(String::new());

    if preview.resources.is_empty() {
        lines.push("  (no resources detected in preview)".to_string());
    } else {
        for (i, resource) in preview.resources.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, resource));
        }
    }

    if preview.requires_confirmation {
        lines.push(String::new());
        lines.push("Confirmation required before proceeding.".to_string());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-ONBOARD-026
    #[test]
    fn parse_empty_output() {
        let resources = parse_preview_output("");
        assert!(resources.is_empty());
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn parse_progress_events() {
        let ndjson = r#"{"type":"progress","resource":"gcp:kms:KeyRing","name":"aegis-keyring","operation":"create","status":"pending"}
{"type":"progress","resource":"gcp:compute:Network","name":"aegis-vpc","operation":"create","status":"pending"}"#;
        let resources = parse_preview_output(ndjson);
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0], "create gcp:kms:KeyRing (aegis-keyring)");
        assert_eq!(resources[1], "create gcp:compute:Network (aegis-vpc)");
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn parse_progress_without_name() {
        let ndjson = r#"{"type":"progress","resource":"gcp:kms:KeyRing","operation":"create","status":"pending"}"#;
        let resources = parse_preview_output(ndjson);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0], "create gcp:kms:KeyRing");
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn parse_diagnostic_events() {
        let ndjson =
            r#"{"type":"diagnostic","severity":"info","message":"Entering state: PREFLIGHT"}"#;
        let resources = parse_preview_output(ndjson);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0], "[info] Entering state: PREFLIGHT");
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn parse_mixed_events() {
        let ndjson = r#"{"type":"diagnostic","severity":"info","message":"Starting preview"}
not json
{"type":"progress","resource":"gcp:kms:KeyRing","operation":"create","status":"pending"}
{"type":"result","success":true}
{"type":"progress","resource":"gcp:compute:Network","operation":"create","status":"pending"}"#;
        let resources = parse_preview_output(ndjson);
        // diagnostic + 2 progress (result type is ignored)
        assert_eq!(resources.len(), 3);
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn parse_skips_malformed_json() {
        let ndjson = "not json\n{invalid}\n";
        let resources = parse_preview_output(ndjson);
        assert!(resources.is_empty());
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn build_preview_from_ndjson() {
        let ndjson = r#"{"type":"progress","resource":"gcp:kms:KeyRing","name":"aegis-keyring","operation":"create","status":"pending"}"#;
        let preview = build_preview("gcp-assured-workloads", ndjson);
        assert_eq!(preview.plugin_name, "gcp-assured-workloads");
        assert_eq!(preview.resources.len(), 1);
        assert!(preview.requires_confirmation);
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn format_preview_with_resources() {
        let preview = PluginPreview {
            plugin_name: "gcp-assured-workloads".to_string(),
            resources: vec![
                "create gcp:kms:KeyRing (aegis-keyring)".to_string(),
                "create gcp:compute:Network (aegis-vpc)".to_string(),
            ],
            requires_confirmation: true,
        };
        let output = format_preview_for_display(&preview);
        assert!(output.contains("gcp-assured-workloads"));
        assert!(output.contains("1. create gcp:kms:KeyRing"));
        assert!(output.contains("2. create gcp:compute:Network"));
        assert!(output.contains("Confirmation required"));
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn format_preview_empty_resources() {
        let preview = PluginPreview {
            plugin_name: "test-plugin".to_string(),
            resources: vec![],
            requires_confirmation: true,
        };
        let output = format_preview_for_display(&preview);
        assert!(output.contains("no resources detected"));
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn format_preview_no_confirmation() {
        let preview = PluginPreview {
            plugin_name: "test-plugin".to_string(),
            resources: vec!["create something".to_string()],
            requires_confirmation: false,
        };
        let output = format_preview_for_display(&preview);
        assert!(
            !output.contains("Confirmation required"),
            "Should not show confirmation when not required"
        );
    }

    // @req REQ-ONBOARD-026
    #[test]
    fn preview_error_display() {
        let err = PreviewError::PluginNotFound("missing.bin".into());
        assert_eq!(format!("{err}"), "plugin not found: missing.bin");

        let err = PreviewError::PluginFailed("exit 1".into());
        assert_eq!(format!("{err}"), "plugin preview failed: exit 1");

        let err = PreviewError::ParseError("bad json".into());
        assert_eq!(format!("{err}"), "preview parse error: bad json");
    }

    // @req REQ-ONBOARD-026
    #[tokio::test]
    async fn get_plugin_preview_rejects_missing_binary() {
        let result = get_plugin_preview(Path::new("/nonexistent/plugin")).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, PreviewError::PluginNotFound(_)),
            "Should return PluginNotFound: {err}"
        );
    }
}
