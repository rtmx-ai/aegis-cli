//! Real-time syslog and HTTPS forwarding transports.
//!
//! Provides [`SyslogPoster`] (RFC 5424 framing over UDP/TCP/TLS) and
//! [`HttpsPoster`] (NDJSON over HTTPS). Both plug into the shared log
//! forwarder as concrete [`BatchPoster`] implementations.
//!
//! REQ-AUDIT-012 -- real-time syslog / HTTPS forwarding
//!
//! # Syslog frame format (RFC 5424)
//!
//! Each [`LedgerEntry`] is rendered as a single syslog message:
//!
//! ```text
//! <PRI>VERSION TIMESTAMP HOSTNAME APP-NAME PROCID MSGID STRUCTURED-DATA MSG
//! ```
//!
//! Where:
//!
//! * `PRI` is `facility * 8 + severity`. Facility is configured on the
//!   [`SyslogPoster`]; severity defaults to `6` (informational) because
//!   the ledger does not currently encode per-entry severity.
//! * `VERSION` is `1`.
//! * `TIMESTAMP` is ISO-8601 from [`LedgerEntry::timestamp`].
//! * `HOSTNAME` and `APP-NAME` are taken from the poster configuration.
//! * `PROCID`, `MSGID`, and `STRUCTURED-DATA` are set to the NIL marker
//!   `-` per RFC 5424.
//! * `MSG` is the JSON-serialized [`LedgerEntry`] (single line).
//!
//! UDP framing is a single datagram per message (RFC 5426). TCP framing
//! uses "non-transparent" line framing with trailing `\n` (RFC 6587). TLS
//! framing matches TCP and is delegated to [`tokio_rustls`] in a future
//! patch; the current implementation treats [`SyslogTransport::Tls`] as
//! an unimplemented placeholder so operators cannot silently fall back
//! to plaintext.

use crate::forwarding::{BatchPoster, LedgerEntry, PostError};
use async_trait::async_trait;
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};

/// Default RFC 5424 severity for audit entries: 6 (informational).
const DEFAULT_SEVERITY: u8 = 6;

/// Transport for [`SyslogPoster`].
#[derive(Debug, Clone)]
pub enum SyslogTransport {
    /// RFC 5426 syslog over UDP.
    Udp {
        /// Destination address of the syslog collector.
        addr: SocketAddr,
    },
    /// RFC 6587 syslog over TCP (non-transparent framing, `\n` delimited).
    Tcp {
        /// Destination address of the syslog collector.
        addr: SocketAddr,
    },
    /// RFC 5425 syslog over TLS. Currently unimplemented -- returns
    /// [`PostError`] when used, so fallback to plaintext is impossible.
    Tls {
        /// Destination address of the syslog collector.
        addr: SocketAddr,
    },
}

/// Real-time syslog transport.
///
/// Construct with a [`SyslogTransport`], facility code, hostname, and
/// application name. Each call to [`BatchPoster::post`] sends one syslog
/// message per entry in the batch.
#[derive(Debug, Clone)]
pub struct SyslogPoster {
    transport: SyslogTransport,
    facility: u8,
    hostname: String,
    app_name: String,
}

impl SyslogPoster {
    /// Build a new poster.
    pub fn new(
        transport: SyslogTransport,
        facility: u8,
        hostname: String,
        app_name: String,
    ) -> Self {
        Self {
            transport,
            facility,
            hostname,
            app_name,
        }
    }

    /// Format a single [`LedgerEntry`] into an RFC 5424 message string.
    ///
    /// Exposed (`pub(crate)`) so unit tests can inspect the rendered
    /// frame without going through the network transport.
    pub(crate) fn format_rfc5424(&self, entry: &LedgerEntry) -> Result<String, PostError> {
        let pri = u16::from(self.facility) * 8 + u16::from(DEFAULT_SEVERITY);
        let timestamp = entry.timestamp.to_rfc3339();
        let msg = serde_json::to_string(entry)
            .map_err(|e| PostError::new(format!("syslog serialize: {e}")))?;
        Ok(format!(
            "<{pri}>1 {ts} {host} {app} - - - {msg}",
            ts = timestamp,
            host = self.hostname,
            app = self.app_name,
        ))
    }

    async fn send_udp(&self, addr: SocketAddr, frames: &[String]) -> Result<(), PostError> {
        let bind = match addr {
            SocketAddr::V4(_) => "0.0.0.0:0",
            SocketAddr::V6(_) => "[::]:0",
        };
        let socket = UdpSocket::bind(bind)
            .await
            .map_err(|e| PostError::new(format!("syslog udp bind: {e}")))?;
        for frame in frames {
            socket
                .send_to(frame.as_bytes(), addr)
                .await
                .map_err(|e| PostError::new(format!("syslog udp send_to: {e}")))?;
        }
        Ok(())
    }

    async fn send_tcp(&self, addr: SocketAddr, frames: &[String]) -> Result<(), PostError> {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| PostError::new(format!("syslog tcp connect: {e}")))?;
        for frame in frames {
            stream
                .write_all(frame.as_bytes())
                .await
                .map_err(|e| PostError::new(format!("syslog tcp write: {e}")))?;
            stream
                .write_all(b"\n")
                .await
                .map_err(|e| PostError::new(format!("syslog tcp write newline: {e}")))?;
        }
        stream
            .shutdown()
            .await
            .map_err(|e| PostError::new(format!("syslog tcp shutdown: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl BatchPoster for SyslogPoster {
    async fn post(&self, _endpoint: &str, batch: &[LedgerEntry]) -> Result<(), PostError> {
        if batch.is_empty() {
            return Ok(());
        }
        let frames: Vec<String> = batch
            .iter()
            .map(|e| self.format_rfc5424(e))
            .collect::<Result<Vec<_>, _>>()?;

        match &self.transport {
            SyslogTransport::Udp { addr } => self.send_udp(*addr, &frames).await,
            SyslogTransport::Tcp { addr } => self.send_tcp(*addr, &frames).await,
            SyslogTransport::Tls { addr: _ } => Err(PostError::new(
                "syslog TLS transport (RFC 5425) not yet implemented; \
                 operators must not silently fall back to plaintext",
            )),
        }
    }
}

/// HTTPS forwarder emitting NDJSON (`application/x-ndjson`).
#[derive(Debug, Clone)]
pub struct HttpsPoster {
    client: reqwest::Client,
    endpoint: String,
}

impl HttpsPoster {
    /// Build a new HTTPS poster targeting `endpoint` with a default
    /// [`reqwest::Client`].
    pub fn new(endpoint: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint,
        }
    }

    /// Build from an existing shared [`reqwest::Client`].
    pub fn with_client(client: reqwest::Client, endpoint: String) -> Self {
        Self { client, endpoint }
    }

    fn build_ndjson_body(batch: &[LedgerEntry]) -> Result<String, PostError> {
        let mut out = String::new();
        for entry in batch {
            let line = serde_json::to_string(entry)
                .map_err(|e| PostError::new(format!("https serialize: {e}")))?;
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }
}

#[async_trait]
impl BatchPoster for HttpsPoster {
    async fn post(&self, _endpoint: &str, batch: &[LedgerEntry]) -> Result<(), PostError> {
        if batch.is_empty() {
            return Ok(());
        }
        let body = Self::build_ndjson_body(batch)?;
        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/x-ndjson")
            .body(body)
            .send()
            .await
            .map_err(|e| PostError::new(format!("https post: {e}")))?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            Err(PostError::new(format!(
                "https endpoint returned {status}: {body}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn entry() -> LedgerEntry {
        LedgerEntry::new(
            Utc.with_ymd_and_hms(2026, 4, 16, 12, 0, 0).unwrap(),
            "alice",
            "host-a",
            serde_json::json!({"SessionStarted": {}}),
            None,
        )
    }

    // rtmx:req REQ-AUDIT-012
    #[test]
    fn rfc5424_pri_is_facility_times_8_plus_severity() {
        let poster = SyslogPoster::new(
            SyslogTransport::Udp {
                addr: "127.0.0.1:0".parse().unwrap(),
            },
            16, // local0
            "h".into(),
            "aegis".into(),
        );
        let rendered = poster.format_rfc5424(&entry()).expect("render");
        assert!(rendered.starts_with("<134>1 "), "got `{rendered}`");
    }

    // rtmx:req REQ-AUDIT-012
    #[test]
    fn rfc5424_contains_hostname_and_app_name() {
        let poster = SyslogPoster::new(
            SyslogTransport::Udp {
                addr: "127.0.0.1:0".parse().unwrap(),
            },
            1,
            "my-host".into(),
            "my-app".into(),
        );
        let rendered = poster.format_rfc5424(&entry()).expect("render");
        assert!(rendered.contains(" my-host my-app - - - "));
    }

    // rtmx:req REQ-AUDIT-012
    #[test]
    fn https_body_is_newline_delimited_json() {
        let batch = vec![entry(), entry()];
        let body = HttpsPoster::build_ndjson_body(&batch).expect("render");
        let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let v: serde_json::Value = serde_json::from_str(line).expect("json");
            assert_eq!(v["os_user"].as_str(), Some("alice"));
        }
    }

    // rtmx:req REQ-AUDIT-012
    #[tokio::test]
    async fn tls_transport_returns_error_until_implemented() {
        let poster = SyslogPoster::new(
            SyslogTransport::Tls {
                addr: "127.0.0.1:6514".parse().unwrap(),
            },
            16,
            "h".into(),
            "aegis".into(),
        );
        let err = poster.post("ignored", &[entry()]).await.unwrap_err();
        assert!(
            err.message.contains("TLS"),
            "TLS must fail closed: got `{}`",
            err.message
        );
    }
}
