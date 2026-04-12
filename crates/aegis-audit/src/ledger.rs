//! Immutable JSONL audit ledger.
//!
//! Appends domain events as single JSON lines to a log file.
//! Metadata only -- no CUI content is ever written.

use aegis_domain::error::DomainError;
use aegis_domain::event::DomainEvent;
use aegis_domain::ports::AuditLedger;
use async_trait::async_trait;
use fs2::FileExt;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A ledger entry wrapping a domain event with identity metadata.
#[derive(Debug, Serialize)]
struct LedgerEntry<'a> {
    timestamp: String,
    os_user: String,
    hostname: String,
    event: &'a DomainEvent,
}

/// Maximum log file size before rotation (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// File-backed JSONL audit ledger with date and size rotation.
///
/// Uses OS-level exclusive file locking (flock on Unix, LockFileEx on
/// Windows) via the `fs2` crate to guarantee concurrent write safety
/// across threads and processes (REQ-AUDIT-007).
pub struct JsonlLedger {
    log_dir: PathBuf,
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

        Ok(Self {
            log_dir: log_dir.to_path_buf(),
        })
    }

    /// Build the current log file path based on today's date.
    fn current_log_path(log_dir: &Path) -> PathBuf {
        let date = chrono::Utc::now().format("%Y-%m-%d");
        log_dir.join(format!("aegis-{date}.jsonl"))
    }

    /// Open the log file in append mode. If the current file exceeds
    /// `MAX_FILE_SIZE`, a new file with a numeric suffix is created.
    fn open_log_file(log_dir: &Path) -> Result<std::fs::File, DomainError> {
        let path = Self::current_log_path(log_dir);

        // Check if we need size-based rotation
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.len() >= MAX_FILE_SIZE
        {
            for i in 1.. {
                let date = chrono::Utc::now().format("%Y-%m-%d");
                let rotated = log_dir.join(format!("aegis-{date}.{i}.jsonl"));
                if !rotated.exists()
                    || std::fs::metadata(&rotated)
                        .map(|m| m.len() < MAX_FILE_SIZE)
                        .unwrap_or(true)
                {
                    return std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&rotated)
                        .map_err(|e| DomainError::AuditError {
                            message: format!(
                                "Failed to open rotated log {}: {e}",
                                rotated.display()
                            ),
                        });
                }
            }
        }

        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
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
        tracing::debug!("audit event recorded");
        let (os_user, hostname) = Self::current_identity();
        let entry = LedgerEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            os_user,
            hostname,
            event,
        };

        // Serialize before acquiring any lock to minimize hold time.
        let mut line = serde_json::to_string(&entry).map_err(|e| DomainError::AuditError {
            message: format!("Failed to serialize ledger entry: {e}"),
        })?;
        line.push('\n');

        let log_dir = self.log_dir.clone();

        // File I/O with exclusive lock runs in a blocking task so we
        // do not block the async runtime (REQ-AUDIT-007).
        tokio::task::spawn_blocking(move || {
            // Use a separate .lock file for cross-platform locking.
            // On Windows, locking the data file itself with LockFileEx
            // returns "Access is denied" when multiple handles compete.
            let lock_path = Self::current_log_path(&log_dir).with_extension("jsonl.lock");
            let lock_file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&lock_path)
                .map_err(|e| DomainError::AuditError {
                    message: format!("Failed to open lock file: {e}"),
                })?;

            // Acquire OS-level exclusive lock (flock on Unix,
            // LockFileEx on Windows). Blocks until available.
            lock_file
                .lock_exclusive()
                .map_err(|e| DomainError::AuditError {
                    message: format!("Failed to acquire exclusive lock: {e}"),
                })?;

            let result = (|| {
                let mut file = Self::open_log_file(&log_dir)?;
                file.write_all(line.as_bytes())
                    .map_err(|e| DomainError::AuditError {
                        message: format!("Failed to write ledger entry: {e}"),
                    })?;
                file.flush().map_err(|e| DomainError::AuditError {
                    message: format!("Failed to flush ledger: {e}"),
                })
            })();

            // Explicitly unlock; also released on drop.
            let _ = lock_file.unlock();

            result
        })
        .await
        .map_err(|e| DomainError::AuditError {
            message: format!("Blocking ledger task panicked: {e}"),
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::event::DomainEvent;
    use aegis_domain::types::*;
    use chrono::Utc;
    use std::sync::Arc;
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

    // @req REQ-AUDIT-004
    #[tokio::test]
    async fn ledger_has_max_file_size_constant() {
        // Verify the rotation threshold is 10 MB
        assert_eq!(super::MAX_FILE_SIZE, 10 * 1024 * 1024);
    }

    // @req REQ-AUDIT-004
    #[tokio::test]
    async fn new_ledger_creates_dated_file() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = make_ledger(&log_dir).await;

        ledger.record(&session_started_event()).await.unwrap();

        let files: Vec<_> = std::fs::read_dir(&log_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .collect();
        assert_eq!(files.len(), 1, "Should have exactly one log file");
    }

    // @req REQ-AUDIT-007
    #[tokio::test]
    async fn concurrent_threads_produce_valid_jsonl() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = Arc::new(make_ledger(&log_dir).await);

        let num_tasks = 10;
        let writes_per_task = 20;
        let mut handles = Vec::new();

        for _ in 0..num_tasks {
            let ledger = Arc::clone(&ledger);
            handles.push(tokio::spawn(async move {
                for _ in 0..writes_per_task {
                    ledger.record(&session_started_event()).await.unwrap();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let entries = read_log_entries(&log_dir);
        assert_eq!(
            entries.len(),
            num_tasks * writes_per_task,
            "All concurrent writes should be recorded"
        );

        // Every line must be valid JSON
        for (i, line) in entries.iter().enumerate() {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("Line {i} is not valid JSON: {e}\nLine: {line}"));
        }
    }

    // @req REQ-AUDIT-007
    #[tokio::test]
    async fn concurrent_writes_no_partial_lines() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = Arc::new(make_ledger(&log_dir).await);

        let mut handles = Vec::new();
        for _ in 0..8 {
            let ledger = Arc::clone(&ledger);
            handles.push(tokio::spawn(async move {
                for _ in 0..25 {
                    ledger.record(&tool_proposed_event()).await.unwrap();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let entries = read_log_entries(&log_dir);
        for (i, line) in entries.iter().enumerate() {
            let parsed: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("Partial/corrupted line {i}: {e}\nLine: {line}"));
            assert!(
                parsed.get("timestamp").is_some(),
                "Line {i} missing timestamp"
            );
            assert!(parsed.get("event").is_some(), "Line {i} missing event");
        }
    }

    // @req REQ-AUDIT-007
    #[tokio::test]
    async fn file_lock_does_not_deadlock() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = Arc::new(make_ledger(&log_dir).await);

        // If locking deadlocks, this test will time out.
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut handles = Vec::new();
            for _ in 0..4 {
                let ledger = Arc::clone(&ledger);
                handles.push(tokio::spawn(async move {
                    for _ in 0..10 {
                        ledger.record(&session_started_event()).await.unwrap();
                    }
                }));
            }
            for h in handles {
                h.await.unwrap();
            }
        })
        .await;

        assert!(result.is_ok(), "Concurrent locking should not deadlock");
    }

    // @req REQ-AUDIT-007
    #[tokio::test]
    async fn single_thread_writes_still_work() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let ledger = make_ledger(&log_dir).await;

        ledger.record(&session_started_event()).await.unwrap();
        ledger.record(&tool_proposed_event()).await.unwrap();
        ledger.record(&tool_approved_event()).await.unwrap();

        let entries = read_log_entries(&log_dir);
        assert_eq!(entries.len(), 3, "Single-thread writes must still work");

        for line in &entries {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("Each single-thread entry must be valid JSON");
        }
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
