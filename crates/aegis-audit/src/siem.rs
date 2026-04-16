//! SIEM export transports for the async log forwarder.
//!
//! Provides a single [`SiemPoster`] [`BatchPoster`] implementation with
//! provider-specific wire formats for Splunk HEC, Elasticsearch Bulk,
//! and Datadog Logs. The endpoint is stored on the poster itself; the
//! `endpoint` argument passed through [`BatchPoster::post`] by the
//! forwarder is ignored so operators can configure multiple posters
//! independently of the shared [`crate::forwarding::ForwarderConfig`].
//!
//! REQ-AUDIT-011 -- SIEM export (Splunk HEC / Elastic Bulk / Datadog Logs)
//!
//! # Wire formats
//!
//! * **Splunk HEC.** `POST {endpoint}/services/collector` with
//!   `Authorization: Splunk {hec_token}`. Body is newline-delimited JSON,
//!   one `{"event": <entry>}` object per line.
//! * **Elasticsearch Bulk.** `POST {endpoint}/_bulk` with
//!   `Content-Type: application/x-ndjson`. Body alternates an action
//!   line `{"index":{"_index":"<index>"}}` and the document line, one
//!   pair per entry.
//! * **Datadog Logs.** `POST {endpoint}/api/v2/logs` with
//!   `DD-API-KEY: {api_key}`. Body is a JSON array of
//!   `{ddsource, message, host, service}` objects.
//!
//! Errors (non-2xx response, connection failure, serialization failure)
//! are surfaced as [`crate::forwarding::PostError`] so the shared
//! forwarder's retry/backoff loop (REQ-AUDIT-020) can handle them.

use crate::forwarding::{BatchPoster, LedgerEntry, PostError};
use async_trait::async_trait;
use serde_json::json;

/// Which SIEM vendor this poster formats for.
///
/// Each variant carries the provider-specific credential or index/source
/// configuration required to shape outbound requests.
#[derive(Debug, Clone)]
pub enum SiemProvider {
    /// Splunk HTTP Event Collector. Requires a pre-provisioned HEC
    /// token; the token is sent in the `Authorization` header.
    SplunkHec {
        /// Splunk HEC token (preshared secret).
        hec_token: String,
    },
    /// Elasticsearch `_bulk` API. Entries are written to `index`.
    ElasticBulk {
        /// Target Elasticsearch index (e.g., `aegis-audit-2026.04`).
        index: String,
    },
    /// Datadog Logs intake API v2.
    DatadogLogs {
        /// Datadog API key, sent in the `DD-API-KEY` header.
        api_key: String,
        /// Logical source label recorded alongside each log line.
        source: String,
    },
}

/// HTTP-based SIEM forwarder.
///
/// Construct with a base endpoint URL and a [`SiemProvider`]. The poster
/// owns a shared [`reqwest::Client`] and is cheap to clone-by-`Arc`.
#[derive(Debug, Clone)]
pub struct SiemPoster {
    client: reqwest::Client,
    endpoint: String,
    provider: SiemProvider,
}

impl SiemPoster {
    /// Create a poster targeting `endpoint` with the given provider
    /// configuration. The [`reqwest::Client`] is built with default TLS.
    pub fn new(endpoint: String, provider: SiemProvider) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint,
            provider,
        }
    }

    /// Build a poster from an existing [`reqwest::Client`]. Useful when
    /// the composition root wants to share a single client across all
    /// outbound destinations.
    pub fn with_client(
        client: reqwest::Client,
        endpoint: String,
        provider: SiemProvider,
    ) -> Self {
        Self {
            client,
            endpoint,
            provider,
        }
    }

    fn build_splunk_body(batch: &[LedgerEntry]) -> Result<String, PostError> {
        let mut out = String::new();
        for entry in batch {
            let line = serde_json::to_string(&json!({ "event": entry }))
                .map_err(|e| PostError::new(format!("splunk serialize: {e}")))?;
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }

    fn build_elastic_body(batch: &[LedgerEntry], index: &str) -> Result<String, PostError> {
        let mut out = String::new();
        for entry in batch {
            let action = serde_json::to_string(&json!({
                "index": { "_index": index }
            }))
            .map_err(|e| PostError::new(format!("elastic action serialize: {e}")))?;
            let doc = serde_json::to_string(entry)
                .map_err(|e| PostError::new(format!("elastic doc serialize: {e}")))?;
            out.push_str(&action);
            out.push('\n');
            out.push_str(&doc);
            out.push('\n');
        }
        Ok(out)
    }

    fn build_datadog_body(batch: &[LedgerEntry], source: &str) -> Result<String, PostError> {
        let items: Vec<serde_json::Value> = batch
            .iter()
            .map(|entry| {
                let message = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
                json!({
                    "ddsource": source,
                    "message": message,
                    "host": entry.hostname,
                    "service": "aegis",
                })
            })
            .collect();
        serde_json::to_string(&items)
            .map_err(|e| PostError::new(format!("datadog serialize: {e}")))
    }
}

#[async_trait]
impl BatchPoster for SiemPoster {
    async fn post(&self, _endpoint: &str, batch: &[LedgerEntry]) -> Result<(), PostError> {
        if batch.is_empty() {
            return Ok(());
        }

        let response = match &self.provider {
            SiemProvider::SplunkHec { hec_token } => {
                let url = format!("{}/services/collector", self.endpoint.trim_end_matches('/'));
                let body = Self::build_splunk_body(batch)?;
                self.client
                    .post(&url)
                    .header("Authorization", format!("Splunk {hec_token}"))
                    .header("Content-Type", "application/json")
                    .body(body)
                    .send()
                    .await
                    .map_err(|e| PostError::new(format!("splunk http: {e}")))?
            }
            SiemProvider::ElasticBulk { index } => {
                let url = format!("{}/_bulk", self.endpoint.trim_end_matches('/'));
                let body = Self::build_elastic_body(batch, index)?;
                self.client
                    .post(&url)
                    .header("Content-Type", "application/x-ndjson")
                    .body(body)
                    .send()
                    .await
                    .map_err(|e| PostError::new(format!("elastic http: {e}")))?
            }
            SiemProvider::DatadogLogs { api_key, source } => {
                let url = format!("{}/api/v2/logs", self.endpoint.trim_end_matches('/'));
                let body = Self::build_datadog_body(batch, source)?;
                self.client
                    .post(&url)
                    .header("DD-API-KEY", api_key.clone())
                    .header("Content-Type", "application/json")
                    .body(body)
                    .send()
                    .await
                    .map_err(|e| PostError::new(format!("datadog http: {e}")))?
            }
        };

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            Err(PostError::new(format!(
                "siem endpoint returned {status}: {body}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry() -> LedgerEntry {
        LedgerEntry::new(
            Utc::now(),
            "tester",
            "host",
            serde_json::json!({"SessionStarted": {}}),
            None,
        )
    }

    // rtmx:req REQ-AUDIT-011
    #[test]
    fn splunk_body_is_newline_delimited_event_objects() {
        let batch = vec![entry(), entry()];
        let body = SiemPoster::build_splunk_body(&batch).expect("serialize");
        let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let v: serde_json::Value = serde_json::from_str(line).expect("json");
            assert!(v.get("event").is_some());
        }
    }

    // rtmx:req REQ-AUDIT-011
    #[test]
    fn elastic_body_alternates_action_and_document() {
        let batch = vec![entry()];
        let body = SiemPoster::build_elastic_body(&batch, "idx-1").expect("serialize");
        let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2);
        let action: serde_json::Value = serde_json::from_str(lines[0]).expect("json");
        assert_eq!(action["index"]["_index"].as_str(), Some("idx-1"));
    }

    // rtmx:req REQ-AUDIT-011
    #[test]
    fn datadog_body_is_json_array_with_required_fields() {
        let batch = vec![entry()];
        let body = SiemPoster::build_datadog_body(&batch, "aegis").expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&body).expect("json");
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["ddsource"].as_str(), Some("aegis"));
        assert_eq!(arr[0]["service"].as_str(), Some("aegis"));
        assert!(arr[0].get("message").is_some());
        assert!(arr[0].get("host").is_some());
    }
}
