//! Immutable JSONL audit ledger.
//!
//! Appends domain events as single JSON lines to a log file.
//! Metadata only -- no CUI content is ever written.

use aegis_domain::error::DomainError;
use aegis_domain::event::DomainEvent;
use aegis_domain::ports::AuditLedger;
use async_trait::async_trait;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// A ledger entry wrapping a domain event with identity metadata.
#[derive(Debug, Serialize)]
struct LedgerEntry<'a> {
    timestamp: String,
    os_user: String,
    hostname: String,
    event: &'a DomainEvent,
}

/// File-backed JSONL audit ledger.
pub struct JsonlLedger {
    log_dir: PathBuf,
    writer: Mutex<Option<tokio::fs::File>>,
}

impl JsonlLedger {
    /// Create a new ledger writing to the given directory.
    /// The directory is created if it does not exist.
    pub async fn new(log_dir: &Path) -> Result<Self, DomainError> {
        tokio::fs::create_dir_all(log_dir)
            .await
            .map_err(|e| DomainError::AuditError {
                message: format!("Failed to create log dir {}: {e}", log_dir.display()),
            })?;

        let ledger = Self {
            log_dir: log_dir.to_path_buf(),
            writer: Mutex::new(None),
        };
        Ok(ledger)
    }

    /// Get or create the current log file.
    async fn ensure_writer(&self) -> Result<tokio::fs::File, DomainError> {
        let date = chrono::Utc::now().format("%Y-%m-%d");
        let path = self.log_dir.join(format!("aegis-{date}.jsonl"));

        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|e| DomainError::AuditError {
                message: format!("Failed to open log file {}: {e}", path.display()),
            })
    }

    fn current_identity() -> (String, String) {
        let os_user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        (os_user, hostname)
    }
}

#[async_trait]
impl AuditLedger for JsonlLedger {
    async fn record(&self, event: &DomainEvent) -> Result<(), DomainError> {
        let (os_user, hostname) = Self::current_identity();
        let entry = LedgerEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            os_user,
            hostname,
            event,
        };

        let mut line = serde_json::to_string(&entry).map_err(|e| DomainError::AuditError {
            message: format!("Failed to serialize ledger entry: {e}"),
        })?;
        line.push('\n');

        let mut guard = self.writer.lock().await;
        let file = match guard.as_mut() {
            Some(f) => f,
            None => {
                *guard = Some(self.ensure_writer().await?);
                guard.as_mut().unwrap()
            }
        };

        file.write_all(line.as_bytes())
            .await
            .map_err(|e| DomainError::AuditError {
                message: format!("Failed to write ledger entry: {e}"),
            })?;

        file.flush().await.map_err(|e| DomainError::AuditError {
            message: format!("Failed to flush ledger: {e}"),
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::event::DomainEvent;
    use aegis_domain::types::*;
    use chrono::Utc;
    use tempfile::TempDir;

    async fn make_ledger(dir: &Path) -> JsonlLedger {
        JsonlLedger::new(dir).await.unwrap()
    }

    fn session_started_event() -> DomainEvent {
        DomainEvent::SessionStarted {
            session_id: SessionId::new(),
            timestamp: Utc::now(),
        }
    }

    fn tool_proposed_event() -> DomainEvent {
        DomainEvent::ToolCallProposed {
            session_id: SessionId::new(),
            request_id: RequestId::new(),
            tool_call: ToolCall::ReadFile {
                path: FilePath::new_unchecked("src/main.rs"),
            },
            timestamp: Utc::now(),
        }
    }

    fn tool_approved_event() -> DomainEvent {
        DomainEvent::ToolCallApproved {
            session_id: SessionId::new(),
            request_id: RequestId::new(),
            decision: ApprovalDecision::Approved,
            timestamp: Utc::now(),
        }
    }

    // @req REQ-AUDIT-001
    #[tokio::test]
    async fn ledger_creates_log_directory() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        assert!(!log_dir.exists());

        let _ledger = make_ledger(&log_dir).await;
        assert!(log_dir.exists());
    }

    // @req REQ-AUDIT-001
    #[tokio::test]
    async fn ledger_appends_jsonl_entries() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = make_ledger(&log_dir).await;

        ledger.record(&session_started_event()).await.unwrap();
        ledger.record(&tool_proposed_event()).await.unwrap();

        // Read the log file
        let entries: Vec<String> = read_log_entries(&log_dir);
        assert_eq!(entries.len(), 2);
    }

    // @req REQ-AUDIT-001
    #[tokio::test]
    async fn each_entry_is_valid_json() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = make_ledger(&log_dir).await;

        ledger.record(&session_started_event()).await.unwrap();
        ledger.record(&tool_proposed_event()).await.unwrap();
        ledger.record(&tool_approved_event()).await.unwrap();

        for line in read_log_entries(&log_dir) {
            let parsed: serde_json::Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("Invalid JSON: {e}\nLine: {line}"));
            // Every entry has timestamp, os_user, hostname, event
            assert!(parsed.get("timestamp").is_some(), "Missing timestamp");
            assert!(parsed.get("os_user").is_some(), "Missing os_user");
            assert!(parsed.get("hostname").is_some(), "Missing hostname");
            assert!(parsed.get("event").is_some(), "Missing event");
        }
    }

    // @req REQ-AUDIT-001
    #[tokio::test]
    async fn ledger_does_not_contain_file_contents() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = make_ledger(&log_dir).await;

        // Record a read_file tool call -- the ledger should
        // contain the PATH but not the file CONTENTS
        ledger.record(&tool_proposed_event()).await.unwrap();

        let entries = read_log_entries(&log_dir);
        let json = &entries[0];

        // Path should be present (metadata)
        assert!(json.contains("src/main.rs"), "Should contain file path");
        // No content field in ReadFile variant
        assert!(
            !json.contains("fn main"),
            "Should not contain file contents"
        );
    }

    // @req REQ-AUDIT-006
    #[tokio::test]
    async fn ledger_entries_contain_user_identity() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = make_ledger(&log_dir).await;

        ledger.record(&session_started_event()).await.unwrap();

        let entries = read_log_entries(&log_dir);
        let parsed: serde_json::Value = serde_json::from_str(&entries[0]).unwrap();

        let os_user = parsed["os_user"].as_str().unwrap();
        assert!(!os_user.is_empty(), "os_user should not be empty");

        let hostname = parsed["hostname"].as_str().unwrap();
        assert!(!hostname.is_empty(), "hostname should not be empty");
    }

    // @req REQ-AUDIT-001
    #[tokio::test]
    async fn ledger_file_named_by_date() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = make_ledger(&log_dir).await;

        ledger.record(&session_started_event()).await.unwrap();

        let date = chrono::Utc::now().format("%Y-%m-%d");
        let expected = log_dir.join(format!("aegis-{date}.jsonl"));
        assert!(
            expected.exists(),
            "Log file should be named aegis-YYYY-MM-DD.jsonl"
        );
    }

    // @req REQ-AUDIT-001
    #[tokio::test]
    async fn ledger_is_append_only() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");

        // Write one entry, drop ledger
        {
            let ledger = make_ledger(&log_dir).await;
            ledger.record(&session_started_event()).await.unwrap();
        }

        // Create new ledger, write another entry
        {
            let ledger = make_ledger(&log_dir).await;
            ledger.record(&tool_proposed_event()).await.unwrap();
        }

        // Both entries should be present
        let entries = read_log_entries(&log_dir);
        assert_eq!(entries.len(), 2, "Ledger should append, not overwrite");
    }

    /// Read all JSONL entries from all log files in the directory.
    fn read_log_entries(log_dir: &Path) -> Vec<String> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(log_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                let content = std::fs::read_to_string(&path).unwrap();
                for line in content.lines() {
                    if !line.trim().is_empty() {
                        entries.push(line.to_string());
                    }
                }
            }
        }
        entries
    }
}
