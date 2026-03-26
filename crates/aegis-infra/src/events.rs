//! aegis-infra/v1 protocol event types.
//!
//! Mirrors the TypeScript types from @aegis-cli/infra-sdk.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single NDJSON event emitted by a plugin on stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PluginEvent {
    Progress(ProgressEvent),
    Diagnostic(DiagnosticEvent),
    Check(CheckEvent),
    Result(ResultEvent),
}

/// Resource provisioning progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub resource: String,
    #[serde(default)]
    pub name: Option<String>,
    pub operation: String,
    pub status: String,
}

/// Informational or warning message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub severity: String,
    pub message: String,
}

/// Health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckEvent {
    pub name: String,
    pub status: CheckStatus,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Health check status values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Warn,
}

/// Final result of a plugin subcommand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEvent {
    pub success: bool,
    #[serde(default)]
    pub outputs: Option<HashMap<String, String>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

/// Plugin manifest returned by the `manifest` subcommand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub contract: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Parse a single NDJSON line into a PluginEvent.
pub fn parse_event(line: &str) -> Option<PluginEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

/// Parse a manifest JSON string.
pub fn parse_manifest(json: &str) -> std::result::Result<PluginManifest, String> {
    serde_json::from_str(json).map_err(|e| format!("Invalid manifest: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-INFRA-004
    #[test]
    fn parse_progress_event() {
        let line = r#"{"type":"progress","resource":"gcp:kms:KeyRing","name":"aegis-keyring","operation":"create","status":"complete"}"#;
        let event = parse_event(line).unwrap();
        match event {
            PluginEvent::Progress(p) => {
                assert_eq!(p.resource, "gcp:kms:KeyRing");
                assert_eq!(p.name.as_deref(), Some("aegis-keyring"));
                assert_eq!(p.status, "complete");
            }
            other => panic!("Expected Progress, got {other:?}"),
        }
    }

    // @req REQ-INFRA-004
    #[test]
    fn parse_diagnostic_event() {
        let line =
            r#"{"type":"diagnostic","severity":"info","message":"Entering state: PREFLIGHT"}"#;
        let event = parse_event(line).unwrap();
        match event {
            PluginEvent::Diagnostic(d) => {
                assert_eq!(d.severity, "info");
                assert!(d.message.contains("PREFLIGHT"));
            }
            other => panic!("Expected Diagnostic, got {other:?}"),
        }
    }

    // @req REQ-INFRA-004
    #[test]
    fn parse_check_event() {
        let line = r#"{"type":"check","name":"kms_key_active","status":"pass","detail":"Key is ENABLED"}"#;
        let event = parse_event(line).unwrap();
        match event {
            PluginEvent::Check(c) => {
                assert_eq!(c.name, "kms_key_active");
                assert_eq!(c.status, CheckStatus::Pass);
                assert_eq!(c.detail.as_deref(), Some("Key is ENABLED"));
            }
            other => panic!("Expected Check, got {other:?}"),
        }
    }

    // @req REQ-INFRA-004
    #[test]
    fn parse_result_event_success() {
        let line = r#"{"type":"result","success":true,"outputs":{"vertex_endpoint":"us-central1-aiplatform.googleapis.com","vpc_name":"aegis-vpc"}}"#;
        let event = parse_event(line).unwrap();
        match event {
            PluginEvent::Result(r) => {
                assert!(r.success);
                let outputs = r.outputs.unwrap();
                assert_eq!(
                    outputs["vertex_endpoint"],
                    "us-central1-aiplatform.googleapis.com"
                );
            }
            other => panic!("Expected Result, got {other:?}"),
        }
    }

    // @req REQ-INFRA-004
    #[test]
    fn parse_result_event_failure() {
        let line = r#"{"type":"result","success":false,"error":"Quota exceeded"}"#;
        let event = parse_event(line).unwrap();
        match event {
            PluginEvent::Result(r) => {
                assert!(!r.success);
                assert_eq!(r.error.as_deref(), Some("Quota exceeded"));
            }
            other => panic!("Expected Result, got {other:?}"),
        }
    }

    // @req REQ-INFRA-004
    #[test]
    fn parse_empty_line_returns_none() {
        assert!(parse_event("").is_none());
        assert!(parse_event("   ").is_none());
    }

    // @req REQ-INFRA-004
    #[test]
    fn parse_malformed_json_returns_none() {
        assert!(parse_event("{not json}").is_none());
        assert!(parse_event("just text").is_none());
    }

    // @req REQ-INFRA-002
    #[test]
    fn parse_manifest_valid() {
        let json = r#"{
            "name": "gcp-assured-workloads",
            "version": "0.2.0",
            "contract": "aegis-infra/v1",
            "description": "IL4/IL5 boundary"
        }"#;
        let manifest = parse_manifest(json).unwrap();
        assert_eq!(manifest.name, "gcp-assured-workloads");
        assert_eq!(manifest.contract, "aegis-infra/v1");
    }

    // @req REQ-INFRA-002
    #[test]
    fn parse_manifest_invalid() {
        let result = parse_manifest("{bad json}");
        assert!(result.is_err());
    }

    // @req REQ-INFRA-004
    #[test]
    fn check_status_deserializes() {
        assert_eq!(
            serde_json::from_str::<CheckStatus>(r#""pass""#).unwrap(),
            CheckStatus::Pass
        );
        assert_eq!(
            serde_json::from_str::<CheckStatus>(r#""fail""#).unwrap(),
            CheckStatus::Fail
        );
        assert_eq!(
            serde_json::from_str::<CheckStatus>(r#""warn""#).unwrap(),
            CheckStatus::Warn
        );
    }
}
