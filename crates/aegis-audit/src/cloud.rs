//! Cloud audit log sinks for provider-native logging.
//!
//! Defines a `CloudAuditSink` trait that forwards audit entries to
//! GCP Cloud Logging, AWS CloudWatch, or Azure Monitor. Current
//! implementations are stubs that validate and format entries; actual
//! HTTP dispatch is deferred to Phase 2.

use aegis_domain::error::DomainError;
use async_trait::async_trait;

/// Target cloud provider for audit log forwarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    Gcp,
    Aws,
    Azure,
}

/// Trait for forwarding audit entries to a cloud-native logging service.
#[async_trait]
pub trait CloudAuditSink: Send + Sync {
    /// Send an audit entry to the cloud logging backend.
    ///
    /// The entry is a JSON value matching the local JSONL ledger format
    /// (timestamp, os_user, hostname, event).
    async fn send(&self, entry: &serde_json::Value) -> Result<(), DomainError>;

    /// Return the cloud provider this sink targets.
    fn provider(&self) -> CloudProvider;
}

/// GCP Cloud Logging sink (stub -- validates entry, no HTTP dispatch).
pub struct GcpCloudLogging {
    pub project_id: String,
    pub log_name: String,
}

/// AWS CloudWatch Logs sink (stub -- validates entry, no HTTP dispatch).
pub struct AwsCloudWatch {
    pub log_group: String,
    pub log_stream: String,
}

/// Azure Monitor sink (stub -- validates entry, no HTTP dispatch).
pub struct AzureMonitor {
    pub workspace_id: String,
}

/// Format an audit entry for a specific cloud provider.
///
/// Returns a provider-appropriate string representation that would be
/// sent as the log payload.
pub fn format_for_provider(provider: CloudProvider, entry: &serde_json::Value) -> String {
    match provider {
        CloudProvider::Gcp => {
            // GCP Cloud Logging uses structured JSON with a jsonPayload wrapper
            let wrapper = serde_json::json!({
                "jsonPayload": entry,
                "severity": "INFO",
                "logName": "aegis-audit",
            });
            serde_json::to_string(&wrapper).unwrap_or_else(|_| entry.to_string())
        }
        CloudProvider::Aws => {
            // CloudWatch Logs expects a message string with a timestamp
            let timestamp = entry
                .get("timestamp")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");
            let wrapper = serde_json::json!({
                "timestamp": timestamp,
                "message": serde_json::to_string(entry)
                    .unwrap_or_else(|_| entry.to_string()),
            });
            serde_json::to_string(&wrapper).unwrap_or_else(|_| entry.to_string())
        }
        CloudProvider::Azure => {
            // Azure Monitor ingestion API expects records in an array
            let wrapper = serde_json::json!({
                "records": [entry],
                "source": "aegis-audit",
            });
            serde_json::to_string(&wrapper).unwrap_or_else(|_| entry.to_string())
        }
    }
}

/// Validate that an audit entry has the required fields.
fn validate_entry(entry: &serde_json::Value) -> Result<(), DomainError> {
    let required = ["timestamp", "os_user", "hostname", "event"];
    for field in &required {
        if entry.get(*field).is_none() {
            return Err(DomainError::AuditError {
                message: format!("Cloud audit entry missing required field: {field}"),
            });
        }
    }
    Ok(())
}

#[async_trait]
impl CloudAuditSink for GcpCloudLogging {
    async fn send(&self, entry: &serde_json::Value) -> Result<(), DomainError> {
        validate_entry(entry)?;
        // Phase 2: POST to Cloud Logging API
        // https://cloud.google.com/logging/docs/reference/v2/rest/v2/entries/write
        let _formatted = format_for_provider(CloudProvider::Gcp, entry);
        Ok(())
    }

    fn provider(&self) -> CloudProvider {
        CloudProvider::Gcp
    }
}

#[async_trait]
impl CloudAuditSink for AwsCloudWatch {
    async fn send(&self, entry: &serde_json::Value) -> Result<(), DomainError> {
        validate_entry(entry)?;
        // Phase 2: PutLogEvents to CloudWatch Logs
        // https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html
        let _formatted = format_for_provider(CloudProvider::Aws, entry);
        Ok(())
    }

    fn provider(&self) -> CloudProvider {
        CloudProvider::Aws
    }
}

#[async_trait]
impl CloudAuditSink for AzureMonitor {
    async fn send(&self, entry: &serde_json::Value) -> Result<(), DomainError> {
        validate_entry(entry)?;
        // Phase 2: POST to Azure Monitor Data Collection API
        // https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
        let _formatted = format_for_provider(CloudProvider::Azure, entry);
        Ok(())
    }

    fn provider(&self) -> CloudProvider {
        CloudProvider::Azure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_entry() -> serde_json::Value {
        serde_json::json!({
            "timestamp": "2026-03-28T12:00:00Z",
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {
                "SessionStarted": {
                    "session_id": "abc-123",
                    "timestamp": "2026-03-28T12:00:00Z"
                }
            }
        })
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn gcp_sink_accepts_valid_entry() {
        let sink = GcpCloudLogging {
            project_id: "my-project".to_string(),
            log_name: "aegis-audit".to_string(),
        };
        let entry = valid_entry();
        let result = sink.send(&entry).await;
        assert!(result.is_ok());
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn aws_sink_accepts_valid_entry() {
        let sink = AwsCloudWatch {
            log_group: "/aegis/audit".to_string(),
            log_stream: "stream-001".to_string(),
        };
        let entry = valid_entry();
        let result = sink.send(&entry).await;
        assert!(result.is_ok());
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn azure_sink_accepts_valid_entry() {
        let sink = AzureMonitor {
            workspace_id: "ws-abc-123".to_string(),
        };
        let entry = valid_entry();
        let result = sink.send(&entry).await;
        assert!(result.is_ok());
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn sink_rejects_entry_missing_timestamp() {
        let sink = GcpCloudLogging {
            project_id: "my-project".to_string(),
            log_name: "aegis-audit".to_string(),
        };
        let entry = serde_json::json!({
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {}
        });
        let result = sink.send(&entry).await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn sink_rejects_entry_missing_event() {
        let sink = AwsCloudWatch {
            log_group: "/aegis/audit".to_string(),
            log_stream: "stream-001".to_string(),
        };
        let entry = serde_json::json!({
            "timestamp": "2026-03-28T12:00:00Z",
            "os_user": "testuser",
            "hostname": "testhost",
        });
        let result = sink.send(&entry).await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn sink_rejects_entry_missing_os_user() {
        let sink = AzureMonitor {
            workspace_id: "ws-abc-123".to_string(),
        };
        let entry = serde_json::json!({
            "timestamp": "2026-03-28T12:00:00Z",
            "hostname": "testhost",
            "event": {}
        });
        let result = sink.send(&entry).await;
        assert!(result.is_err());
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn format_for_gcp_wraps_in_json_payload() {
        let entry = valid_entry();
        let formatted = format_for_provider(CloudProvider::Gcp, &entry);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert!(parsed.get("jsonPayload").is_some());
        assert_eq!(parsed["severity"], "INFO");
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn format_for_aws_includes_message_and_timestamp() {
        let entry = valid_entry();
        let formatted = format_for_provider(CloudProvider::Aws, &entry);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert!(parsed.get("message").is_some());
        assert_eq!(parsed["timestamp"], "2026-03-28T12:00:00Z");
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn format_for_azure_wraps_in_records_array() {
        let entry = valid_entry();
        let formatted = format_for_provider(CloudProvider::Azure, &entry);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert!(parsed.get("records").is_some());
        assert!(parsed["records"].is_array());
        assert_eq!(parsed["records"].as_array().unwrap().len(), 1);
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn provider_method_returns_correct_variant() {
        let gcp = GcpCloudLogging {
            project_id: "p".to_string(),
            log_name: "l".to_string(),
        };
        assert_eq!(gcp.provider(), CloudProvider::Gcp);

        let aws = AwsCloudWatch {
            log_group: "g".to_string(),
            log_stream: "s".to_string(),
        };
        assert_eq!(aws.provider(), CloudProvider::Aws);

        let azure = AzureMonitor {
            workspace_id: "w".to_string(),
        };
        assert_eq!(azure.provider(), CloudProvider::Azure);
    }

    // rtmx:req REQ-AUDIT-002
    #[tokio::test]
    async fn all_sinks_reject_empty_object() {
        let empty = serde_json::json!({});

        let gcp = GcpCloudLogging {
            project_id: "p".to_string(),
            log_name: "l".to_string(),
        };
        assert!(gcp.send(&empty).await.is_err());

        let aws = AwsCloudWatch {
            log_group: "g".to_string(),
            log_stream: "s".to_string(),
        };
        assert!(aws.send(&empty).await.is_err());

        let azure = AzureMonitor {
            workspace_id: "w".to_string(),
        };
        assert!(azure.send(&empty).await.is_err());
    }
}
