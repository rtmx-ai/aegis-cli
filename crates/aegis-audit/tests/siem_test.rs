//! Integration tests for REQ-AUDIT-011: SIEM export with provider-specific
//! wire formats for Splunk HEC, Elastic Bulk, and Datadog Logs.
//!
//! These tests spin up [`wiremock::MockServer`] instances, drive the
//! [`SiemPoster`] implementation against each mocked endpoint, and assert
//! that the outbound request body and authentication headers match each
//! vendor's documented wire format.

use aegis_audit::forwarding::{BatchPoster, LedgerEntry};
use aegis_audit::siem::{SiemPoster, SiemProvider};
use chrono::{TimeZone, Utc};
use serde_json::Value;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn sample_batch() -> Vec<LedgerEntry> {
    vec![
        LedgerEntry::new(
            Utc.with_ymd_and_hms(2026, 4, 16, 12, 0, 0).unwrap(),
            "alice",
            "host-a",
            serde_json::json!({"SessionStarted": {"session_id": "s1"}}),
            Some("REQ-AUDIT-011".to_string()),
        ),
        LedgerEntry::new(
            Utc.with_ymd_and_hms(2026, 4, 16, 12, 0, 1).unwrap(),
            "bob",
            "host-b",
            serde_json::json!({"SessionEnded": {"session_id": "s1"}}),
            None,
        ),
    ]
}

/// Extract the raw UTF-8 body captured by a wiremock request.
fn request_body_as_str(req: &Request) -> String {
    String::from_utf8(req.body.clone()).expect("request body must be UTF-8")
}

// rtmx:req REQ-AUDIT-011
#[tokio::test]
async fn test_siem_poster_formats_for_each_provider() {
    // ---- Splunk HEC -----------------------------------------------------
    let splunk_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/services/collector"))
        .and(header("Authorization", "Splunk hec-token-xyz"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&splunk_server)
        .await;

    let splunk_poster = SiemPoster::new(
        splunk_server.uri(),
        SiemProvider::SplunkHec {
            hec_token: "hec-token-xyz".into(),
        },
    );
    splunk_poster
        .post("ignored-endpoint", &sample_batch())
        .await
        .expect("splunk post should succeed");

    let received = splunk_server.received_requests().await.expect("requests");
    assert_eq!(received.len(), 1);
    let body = request_body_as_str(&received[0]);
    let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
    assert_eq!(lines.len(), 2, "splunk body should have one line per entry");
    for line in &lines {
        let v: Value = serde_json::from_str(line).expect("splunk line is JSON");
        assert!(v.get("event").is_some(), "each line must have `event` key");
    }

    // ---- Elastic Bulk ---------------------------------------------------
    let elastic_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/_bulk"))
        .and(header("Content-Type", "application/x-ndjson"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&elastic_server)
        .await;

    let elastic_poster = SiemPoster::new(
        elastic_server.uri(),
        SiemProvider::ElasticBulk {
            index: "aegis-audit".into(),
        },
    );
    elastic_poster
        .post("ignored-endpoint", &sample_batch())
        .await
        .expect("elastic post should succeed");

    let received = elastic_server.received_requests().await.expect("requests");
    assert_eq!(received.len(), 1);
    let body = request_body_as_str(&received[0]);
    let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
    assert_eq!(
        lines.len(),
        4,
        "elastic body should alternate action+doc => 4 lines for 2 entries"
    );
    let action_0: Value = serde_json::from_str(lines[0]).expect("elastic action line 0 is JSON");
    assert_eq!(
        action_0["index"]["_index"].as_str(),
        Some("aegis-audit"),
        "action line must reference the configured index"
    );
    let doc_0: Value = serde_json::from_str(lines[1]).expect("elastic doc line 0 is JSON");
    assert_eq!(doc_0["os_user"].as_str(), Some("alice"));

    // ---- Datadog Logs ---------------------------------------------------
    let datadog_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/logs"))
        .and(header("DD-API-KEY", "dd-api-key-abc"))
        .respond_with(ResponseTemplate::new(202))
        .mount(&datadog_server)
        .await;

    let datadog_poster = SiemPoster::new(
        datadog_server.uri(),
        SiemProvider::DatadogLogs {
            api_key: "dd-api-key-abc".into(),
            source: "aegis".into(),
        },
    );
    datadog_poster
        .post("ignored-endpoint", &sample_batch())
        .await
        .expect("datadog post should succeed");

    let received = datadog_server.received_requests().await.expect("requests");
    assert_eq!(received.len(), 1);
    let body = request_body_as_str(&received[0]);
    let parsed: Value = serde_json::from_str(&body).expect("datadog body is JSON");
    let arr = parsed.as_array().expect("datadog body must be an array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["ddsource"].as_str(), Some("aegis"));
    assert_eq!(arr[0]["service"].as_str(), Some("aegis"));
    assert_eq!(arr[0]["host"].as_str(), Some("host-a"));
    assert!(
        arr[0].get("message").is_some(),
        "datadog payload must include `message`"
    );
}

// rtmx:req REQ-AUDIT-011
#[tokio::test]
async fn test_siem_poster_handles_5xx_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/services/collector"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let poster = SiemPoster::new(
        server.uri(),
        SiemProvider::SplunkHec {
            hec_token: "t".into(),
        },
    );
    let result = poster.post("ignored", &sample_batch()).await;
    assert!(
        result.is_err(),
        "5xx response must propagate as Err to trigger retry"
    );
}

// rtmx:req REQ-AUDIT-011
#[tokio::test]
async fn test_siem_poster_handles_auth_failure() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/logs"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let poster = SiemPoster::new(
        server.uri(),
        SiemProvider::DatadogLogs {
            api_key: "bad".into(),
            source: "aegis".into(),
        },
    );
    let result = poster.post("ignored", &sample_batch()).await;
    assert!(
        result.is_err(),
        "401 auth failure must propagate as Err (no point retrying forever but transport layer surfaces it)"
    );
}
