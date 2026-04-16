//! Integration tests for REQ-AUDIT-012: real-time syslog (RFC 5424) and
//! HTTPS forwarding transports.

use aegis_audit::forwarding::{BatchPoster, LedgerEntry};
use aegis_audit::syslog::{HttpsPoster, SyslogPoster, SyslogTransport};
use chrono::{TimeZone, Utc};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_entry() -> LedgerEntry {
    LedgerEntry::new(
        Utc.with_ymd_and_hms(2026, 4, 16, 12, 0, 0).unwrap(),
        "alice",
        "host-a",
        serde_json::json!({"SessionStarted": {"session_id": "s1"}}),
        Some("REQ-AUDIT-012".to_string()),
    )
}

// rtmx:req REQ-AUDIT-012
#[tokio::test]
async fn test_syslog_poster_formats_rfc5424() {
    // Bind an ephemeral UDP listener to capture the packet emitted by
    // SyslogPoster.
    let listener = UdpSocket::bind("127.0.0.1:0").await.expect("bind udp");
    let listener_addr: SocketAddr = listener.local_addr().expect("local_addr");

    let poster = SyslogPoster::new(
        SyslogTransport::Udp {
            addr: listener_addr,
        },
        16, // local0
        "aegis-host".to_string(),
        "aegis".to_string(),
    );

    // Use a fresh tokio task so send and recv run concurrently.
    let recv_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        let (n, _src) =
            tokio::time::timeout(Duration::from_secs(2), listener.recv_from(&mut buf))
                .await
                .expect("udp recv within timeout")
                .expect("udp recv_from");
        String::from_utf8_lossy(&buf[..n]).into_owned()
    });

    poster
        .post("ignored", &[sample_entry()])
        .await
        .expect("syslog udp post should succeed");

    let captured = recv_task.await.expect("recv task join");

    // RFC 5424 header: <PRI>VERSION SP TIMESTAMP SP HOSTNAME SP APP-NAME
    // SP PROCID SP MSGID SP STRUCTURED-DATA SP MSG
    assert!(
        captured.starts_with('<'),
        "syslog frame must start with '<PRI>': got `{captured}`"
    );
    let close = captured.find('>').expect("closing '>' in PRI");
    let pri: u16 = captured[1..close].parse().expect("PRI is an integer");
    // facility=16 (local0) * 8 + severity=6 (info) => 134.
    assert_eq!(pri, 134, "PRI must equal facility*8 + severity");

    let rest = &captured[close + 1..];
    // Version immediately follows '>' and is '1' in RFC 5424.
    assert!(
        rest.starts_with("1 "),
        "RFC 5424 VERSION must be '1': got `{rest}`"
    );

    let parts: Vec<&str> = rest.splitn(7, ' ').collect();
    // parts = [VERSION, TIMESTAMP, HOSTNAME, APP-NAME, PROCID, MSGID, SD+MSG]
    assert_eq!(parts.len(), 7, "expected 7 header fields, got `{rest}`");
    assert_eq!(parts[0], "1");
    // Timestamp should be ISO-8601 with year 2026
    assert!(
        parts[1].starts_with("2026-04-16T12:00:00"),
        "timestamp mismatch: got `{}`",
        parts[1]
    );
    assert_eq!(parts[2], "aegis-host");
    assert_eq!(parts[3], "aegis");
    // PROCID and MSGID may be "-" if not set.
    // STRUCTURED-DATA is followed by a space and then the MSG.
    // MSG must contain the JSON-encoded entry.
    let sd_and_msg = parts[6];
    assert!(
        sd_and_msg.contains("\"os_user\":\"alice\""),
        "msg should include JSON of the entry: got `{sd_and_msg}`"
    );
}

// rtmx:req REQ-AUDIT-012
#[tokio::test]
async fn test_https_poster_posts_jsonl() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ingest"))
        .and(header("Content-Type", "application/x-ndjson"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let endpoint = format!("{}/ingest", server.uri());
    let poster = HttpsPoster::new(endpoint);

    let batch = vec![sample_entry(), sample_entry()];
    poster
        .post("ignored", &batch)
        .await
        .expect("https post should succeed");

    let received = server.received_requests().await.expect("requests");
    assert_eq!(received.len(), 1);
    let body = String::from_utf8(received[0].body.clone()).expect("utf8");
    let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
    assert_eq!(lines.len(), 2, "ndjson body must have one line per entry");
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("each line is JSON");
        assert_eq!(v["os_user"].as_str(), Some("alice"));
    }
}

// rtmx:req REQ-AUDIT-012
#[tokio::test]
async fn test_syslog_poster_handles_unreachable_destination() {
    // Bind and immediately drop a TCP listener to guarantee a known-closed
    // TCP port; connection should fail.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tcp");
    let addr = listener.local_addr().expect("tcp local_addr");
    drop(listener);

    let poster = SyslogPoster::new(
        SyslogTransport::Tcp { addr },
        16,
        "aegis-host".to_string(),
        "aegis".to_string(),
    );
    let result = poster.post("ignored", &[sample_entry()]).await;
    assert!(
        result.is_err(),
        "TCP to a closed port must return Err (connection refused)"
    );
}
