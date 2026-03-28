//! Mock plugin utilities for deterministic integration testing.
//!
//! Provides `MockPluginBuilder` to construct `PluginOutput` values without
//! spawning a real subprocess, plus helpers to create test `PluginManifest`
//! and `Plugin` instances.

use crate::events::{PluginEvent, PluginManifest, ResultEvent};
use crate::host::{Plugin, PluginOutput};
use std::collections::HashMap;
use std::path::PathBuf;

/// Fluent builder for constructing deterministic `PluginOutput` values.
///
/// # Example
///
/// ```
/// use aegis_infra::mock_plugin::MockPluginBuilder;
///
/// let output = MockPluginBuilder::new("test")
///     .with_result(true)
///     .with_exit_code(0)
///     .build();
///
/// assert!(output.result.unwrap().success);
/// assert_eq!(output.exit_code, 0);
/// ```
pub struct MockPluginBuilder {
    _name: String,
    events: Vec<PluginEvent>,
    result_success: Option<bool>,
    result_outputs: Option<HashMap<String, String>>,
    result_error: Option<String>,
    result_summary: Option<String>,
    stderr: String,
    exit_code: i32,
}

impl MockPluginBuilder {
    /// Create a new builder for a mock plugin with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            _name: name.to_string(),
            events: Vec::new(),
            result_success: None,
            result_outputs: None,
            result_error: None,
            result_summary: None,
            stderr: String::new(),
            exit_code: 0,
        }
    }

    /// Append a plugin event to the output stream.
    pub fn with_event(mut self, event: PluginEvent) -> Self {
        self.events.push(event);
        self
    }

    /// Set whether the result event reports success.
    pub fn with_result(mut self, success: bool) -> Self {
        self.result_success = Some(success);
        self
    }

    /// Set the outputs map on the result event.
    pub fn with_result_outputs(mut self, outputs: HashMap<String, String>) -> Self {
        self.result_outputs = Some(outputs);
        self
    }

    /// Set the stderr content.
    pub fn with_stderr(mut self, stderr: &str) -> Self {
        self.stderr = stderr.to_string();
        self
    }

    /// Set the exit code.
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    /// Consume the builder and produce a `PluginOutput`.
    ///
    /// If `with_result()` was called, a `ResultEvent` is generated and
    /// appended to the events list (matching real plugin behavior where
    /// the result event is also part of the NDJSON stream).
    pub fn build(mut self) -> PluginOutput {
        let result = self.result_success.map(|success| ResultEvent {
            success,
            outputs: self.result_outputs.take(),
            error: self.result_error.take(),
            summary: self.result_summary.take(),
        });

        // Append the result event to the events vec, mirroring real plugins.
        if let Some(ref r) = result {
            self.events.push(PluginEvent::Result(r.clone()));
        }

        PluginOutput {
            events: self.events,
            result,
            stderr: self.stderr,
            exit_code: self.exit_code,
        }
    }
}

/// Create a test `PluginManifest` with sensible defaults.
pub fn mock_manifest(name: &str) -> PluginManifest {
    PluginManifest {
        name: name.to_string(),
        version: "0.0.0-mock".to_string(),
        contract: "aegis-infra/v1".to_string(),
        description: Some(format!("Mock plugin: {name}")),
    }
}

/// Create a `Plugin` with a dummy binary path, suitable for unit tests
/// that do not spawn a subprocess.
pub fn mock_plugin(name: &str) -> Plugin {
    Plugin {
        binary: PathBuf::from(format!("/dev/null/{name}")),
        manifest: mock_manifest(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{CheckEvent, CheckStatus, DiagnosticEvent, ProgressEvent};

    // @req REQ-TEST-013
    #[test]
    fn builder_default_output_is_empty_success() {
        let output = MockPluginBuilder::new("empty").build();

        assert!(output.events.is_empty());
        assert!(output.result.is_none());
        assert!(output.stderr.is_empty());
        assert_eq!(output.exit_code, 0);
    }

    // @req REQ-TEST-013
    #[test]
    fn builder_produces_expected_result() {
        let output = MockPluginBuilder::new("basic")
            .with_result(true)
            .with_exit_code(0)
            .build();

        let result = output.result.expect("should have result");
        assert!(result.success);
        assert_eq!(output.exit_code, 0);
    }

    // @req REQ-TEST-013
    #[test]
    fn builder_events_collected_in_order() {
        let output = MockPluginBuilder::new("ordered")
            .with_event(PluginEvent::Diagnostic(DiagnosticEvent {
                severity: "info".to_string(),
                message: "first".to_string(),
            }))
            .with_event(PluginEvent::Progress(ProgressEvent {
                resource: "kms".to_string(),
                name: Some("my-key".to_string()),
                operation: "create".to_string(),
                status: "complete".to_string(),
            }))
            .with_event(PluginEvent::Check(CheckEvent {
                name: "kms_active".to_string(),
                status: CheckStatus::Pass,
                detail: None,
            }))
            .with_result(true)
            .build();

        // 3 explicit events + 1 appended result event = 4
        assert_eq!(output.events.len(), 4);

        match &output.events[0] {
            PluginEvent::Diagnostic(d) => assert_eq!(d.message, "first"),
            other => panic!("Expected Diagnostic, got {other:?}"),
        }
        match &output.events[1] {
            PluginEvent::Progress(p) => assert_eq!(p.resource, "kms"),
            other => panic!("Expected Progress, got {other:?}"),
        }
        match &output.events[2] {
            PluginEvent::Check(c) => {
                assert_eq!(c.name, "kms_active");
                assert_eq!(c.status, CheckStatus::Pass);
            }
            other => panic!("Expected Check, got {other:?}"),
        }
        match &output.events[3] {
            PluginEvent::Result(r) => assert!(r.success),
            other => panic!("Expected Result, got {other:?}"),
        }
    }

    // @req REQ-TEST-013
    #[test]
    fn builder_result_with_outputs() {
        let mut outputs = HashMap::new();
        outputs.insert(
            "vertex_endpoint".to_string(),
            "us-central1-aiplatform.googleapis.com".to_string(),
        );
        outputs.insert("vpc_name".to_string(), "aegis-vpc".to_string());

        let output = MockPluginBuilder::new("with-outputs")
            .with_result(true)
            .with_result_outputs(outputs)
            .build();

        let result = output.result.expect("should have result");
        assert!(result.success);
        let map = result.outputs.expect("should have outputs");
        assert_eq!(
            map["vertex_endpoint"],
            "us-central1-aiplatform.googleapis.com"
        );
        assert_eq!(map["vpc_name"], "aegis-vpc");
    }

    // @req REQ-TEST-013
    #[test]
    fn builder_stderr_and_exit_code_configurable() {
        let output = MockPluginBuilder::new("failing")
            .with_stderr("quota exceeded")
            .with_exit_code(2)
            .with_result(false)
            .build();

        assert_eq!(output.stderr, "quota exceeded");
        assert_eq!(output.exit_code, 2);
        assert!(!output.result.unwrap().success);
    }

    // @req REQ-TEST-013
    #[test]
    fn mock_manifest_has_correct_contract() {
        let manifest = mock_manifest("test-plugin");
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.contract, "aegis-infra/v1");
        assert_eq!(manifest.version, "0.0.0-mock");
        assert!(manifest.description.is_some());
    }

    // @req REQ-TEST-013
    #[test]
    fn mock_plugin_has_dummy_binary() {
        let plugin = mock_plugin("my-plugin");
        assert_eq!(plugin.manifest.name, "my-plugin");
        assert_eq!(plugin.manifest.contract, "aegis-infra/v1");
        assert!(plugin.binary.to_string_lossy().contains("my-plugin"));
    }
}
