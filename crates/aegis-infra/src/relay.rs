//! Event relay: converts plugin events to display-friendly strings.
//!
//! Used by the TUI layer to present plugin progress, diagnostics,
//! health checks, and results to the operator.

use crate::events::*;
use crate::host::PluginOutput;

/// Format any `PluginEvent` into a display-friendly string.
pub fn format_event(event: &PluginEvent) -> String {
    match event {
        PluginEvent::Progress(p) => format_progress(p),
        PluginEvent::Diagnostic(d) => format_diagnostic(d),
        PluginEvent::Check(c) => format_check(c),
        PluginEvent::Result(r) => format_result(r),
    }
}

/// Format a progress event as `[resource] operation: status`.
///
/// If the event includes a `name`, it appears after the resource:
/// `[resource] name operation: status`.
pub fn format_progress(event: &ProgressEvent) -> String {
    match &event.name {
        Some(name) => format!(
            "[{}] {} {}: {}",
            event.resource, name, event.operation, event.status
        ),
        None => format!("[{}] {}: {}", event.resource, event.operation, event.status),
    }
}

/// Format a diagnostic event as `[severity] message`.
pub fn format_diagnostic(event: &DiagnosticEvent) -> String {
    format!("[{}] {}", event.severity, event.message)
}

/// Format a check event as `[status] name` with optional detail.
pub fn format_check(event: &CheckEvent) -> String {
    let status_str = match event.status {
        CheckStatus::Pass => "pass",
        CheckStatus::Fail => "FAIL",
        CheckStatus::Warn => "warn",
    };
    match &event.detail {
        Some(detail) => format!("[{}] {}: {}", status_str, event.name, detail),
        None => format!("[{}] {}", status_str, event.name),
    }
}

/// Format a result event as a success/failure summary.
pub fn format_result(event: &ResultEvent) -> String {
    if event.success {
        match &event.summary {
            Some(summary) => format!("Result: success -- {summary}"),
            None => "Result: success".to_string(),
        }
    } else {
        match &event.error {
            Some(err) => format!("Result: FAILED -- {err}"),
            None => "Result: FAILED".to_string(),
        }
    }
}

/// Convert all events from a plugin run into display-friendly strings.
pub fn relay_output(output: &PluginOutput) -> Vec<String> {
    output.events.iter().map(format_event).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // @req REQ-INFRA-005
    #[test]
    fn format_progress_with_name() {
        let event = ProgressEvent {
            resource: "kms".to_string(),
            name: Some("aegis-keyring".to_string()),
            operation: "create".to_string(),
            status: "complete".to_string(),
        };
        assert_eq!(
            format_progress(&event),
            "[kms] aegis-keyring create: complete"
        );
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_progress_without_name() {
        let event = ProgressEvent {
            resource: "vpc".to_string(),
            name: None,
            operation: "update".to_string(),
            status: "in-progress".to_string(),
        };
        assert_eq!(format_progress(&event), "[vpc] update: in-progress");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_diagnostic_info() {
        let event = DiagnosticEvent {
            severity: "info".to_string(),
            message: "Starting provisioning".to_string(),
        };
        assert_eq!(format_diagnostic(&event), "[info] Starting provisioning");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_diagnostic_warning() {
        let event = DiagnosticEvent {
            severity: "warning".to_string(),
            message: "Quota near limit".to_string(),
        };
        assert_eq!(format_diagnostic(&event), "[warning] Quota near limit");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_check_pass() {
        let event = CheckEvent {
            name: "kms_key_active".to_string(),
            status: CheckStatus::Pass,
            detail: Some("Key is ENABLED".to_string()),
        };
        assert_eq!(
            format_check(&event),
            "[pass] kms_key_active: Key is ENABLED"
        );
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_check_fail_no_detail() {
        let event = CheckEvent {
            name: "vpc_perimeter".to_string(),
            status: CheckStatus::Fail,
            detail: None,
        };
        assert_eq!(format_check(&event), "[FAIL] vpc_perimeter");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_check_warn_with_detail() {
        let event = CheckEvent {
            name: "audit_sink".to_string(),
            status: CheckStatus::Warn,
            detail: Some("Permission denied".to_string()),
        };
        assert_eq!(format_check(&event), "[warn] audit_sink: Permission denied");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_result_success_with_summary() {
        let event = ResultEvent {
            success: true,
            outputs: None,
            error: None,
            summary: Some("8 resources created".to_string()),
        };
        assert_eq!(
            format_result(&event),
            "Result: success -- 8 resources created"
        );
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_result_success_no_summary() {
        let event = ResultEvent {
            success: true,
            outputs: Some(HashMap::new()),
            error: None,
            summary: None,
        };
        assert_eq!(format_result(&event), "Result: success");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_result_failure_with_error() {
        let event = ResultEvent {
            success: false,
            outputs: None,
            error: Some("Quota exceeded".to_string()),
            summary: None,
        };
        assert_eq!(format_result(&event), "Result: FAILED -- Quota exceeded");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_result_failure_no_error() {
        let event = ResultEvent {
            success: false,
            outputs: None,
            error: None,
            summary: None,
        };
        assert_eq!(format_result(&event), "Result: FAILED");
    }

    // @req REQ-INFRA-005
    #[test]
    fn format_event_dispatches_correctly() {
        let progress = PluginEvent::Progress(ProgressEvent {
            resource: "kms".to_string(),
            name: None,
            operation: "create".to_string(),
            status: "complete".to_string(),
        });
        assert_eq!(format_event(&progress), "[kms] create: complete");

        let diag = PluginEvent::Diagnostic(DiagnosticEvent {
            severity: "info".to_string(),
            message: "hello".to_string(),
        });
        assert_eq!(format_event(&diag), "[info] hello");

        let check = PluginEvent::Check(CheckEvent {
            name: "test".to_string(),
            status: CheckStatus::Pass,
            detail: None,
        });
        assert_eq!(format_event(&check), "[pass] test");

        let result = PluginEvent::Result(ResultEvent {
            success: true,
            outputs: None,
            error: None,
            summary: None,
        });
        assert_eq!(format_event(&result), "Result: success");
    }

    // @req REQ-INFRA-005
    #[test]
    fn relay_output_converts_all_events() {
        let output = PluginOutput {
            events: vec![
                PluginEvent::Diagnostic(DiagnosticEvent {
                    severity: "info".to_string(),
                    message: "Starting".to_string(),
                }),
                PluginEvent::Progress(ProgressEvent {
                    resource: "kms".to_string(),
                    name: None,
                    operation: "create".to_string(),
                    status: "complete".to_string(),
                }),
                PluginEvent::Check(CheckEvent {
                    name: "kms_key".to_string(),
                    status: CheckStatus::Pass,
                    detail: Some("OK".to_string()),
                }),
                PluginEvent::Result(ResultEvent {
                    success: true,
                    outputs: None,
                    error: None,
                    summary: Some("Done".to_string()),
                }),
            ],
            result: Some(ResultEvent {
                success: true,
                outputs: None,
                error: None,
                summary: Some("Done".to_string()),
            }),
            stderr: String::new(),
            exit_code: 0,
        };

        let lines = relay_output(&output);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "[info] Starting");
        assert_eq!(lines[1], "[kms] create: complete");
        assert_eq!(lines[2], "[pass] kms_key: OK");
        assert_eq!(lines[3], "Result: success -- Done");
    }

    // @req REQ-INFRA-005
    #[test]
    fn relay_output_empty_events() {
        let output = PluginOutput {
            events: vec![],
            result: None,
            stderr: String::new(),
            exit_code: 0,
        };
        let lines = relay_output(&output);
        assert!(lines.is_empty());
    }
}
