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
    /// Optional RTMX requirement ID for traceability (REQ-AUDIT-003).
    #[serde(skip_serializing_if = "Option::is_none")]
    req_id: Option<String>,
}

/// Report returned by crash recovery describing what was found and repaired.
#[derive(Debug)]
pub struct RecoveryReport {
    /// Number of valid JSONL entries retained in the log file.
    pub valid_entries: usize,
    /// Number of corrupt/truncated entries moved to the quarantine file.
    pub quarantined_entries: usize,
    /// Path to the `.corrupt` quarantine file, if any entries were quarantined.
    pub quarantine_path: Option<PathBuf>,
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

    /// Scan all `.jsonl` log files in `log_dir` for truncated/corrupt entries.
    ///
    /// Valid lines are kept in place; invalid lines are moved to a sibling
    /// `.corrupt` file. A `LEDGER_REPAIRED` entry is appended to each
    /// repaired log file so the repair is itself auditable.
    ///
    /// Returns a [`RecoveryReport`] summarising what was found.
    pub fn recover(log_dir: &Path) -> RecoveryReport {
        let mut total_valid: usize = 0;
        let mut total_quarantined: usize = 0;
        let mut quarantine_path: Option<PathBuf> = None;

        let entries = match std::fs::read_dir(log_dir) {
            Ok(e) => e,
            Err(_) => {
                return RecoveryReport {
                    valid_entries: 0,
                    quarantined_entries: 0,
                    quarantine_path: None,
                };
            }
        };

        for dir_entry in entries.flatten() {
            let path = dir_entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let mut valid_lines: Vec<String> = Vec::new();
                let mut corrupt_lines: Vec<String> = Vec::new();

                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if serde_json::from_str::<serde_json::Value>(line).is_ok() {
                        valid_lines.push(line.to_string());
                    } else {
                        corrupt_lines.push(line.to_string());
                    }
                }

                total_valid += valid_lines.len();

                if !corrupt_lines.is_empty() {
                    total_quarantined += corrupt_lines.len();

                    // Write corrupt lines to .corrupt file
                    let corrupt_path = path.with_extension("jsonl.corrupt");
                    let mut corrupt_content = String::new();
                    for cl in &corrupt_lines {
                        corrupt_content.push_str(cl);
                        corrupt_content.push('\n');
                    }
                    // Append to existing corrupt file if present
                    let mut file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&corrupt_path)
                        .expect("Failed to open corrupt quarantine file");
                    file.write_all(corrupt_content.as_bytes())
                        .expect("Failed to write corrupt entries");
                    quarantine_path = Some(corrupt_path);

                    // Build the LEDGER_REPAIRED entry
                    let (os_user, hostname) = Self::current_identity();
                    let repair_entry = serde_json::json!({
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "os_user": os_user,
                        "hostname": hostname,
                        "event": "LEDGER_REPAIRED",
                        "quarantined_entries": corrupt_lines.len(),
                    });
                    valid_lines.push(
                        serde_json::to_string(&repair_entry)
                            .expect("Failed to serialize repair entry"),
                    );
                    // Count the repair entry as valid
                    total_valid += 1;

                    // Rewrite the log file with only valid lines
                    // + repair entry
                    let mut rewritten = String::new();
                    for vl in &valid_lines {
                        rewritten.push_str(vl);
                        rewritten.push('\n');
                    }
                    std::fs::write(&path, rewritten)
                        .expect("Failed to rewrite repaired log file");
                }
            }
        }

        RecoveryReport {
            valid_entries: total_valid,
            quarantined_entries: total_quarantined,
            quarantine_path,
        }
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
        self.record_with_req(event, None).await
    }

    async fn record_with_req(
        &self,
        event: &DomainEvent,
        req_id: Option<&str>,
    ) -> Result<(), DomainError> {
        tracing::debug!(req_id = req_id, "audit event recorded");
        let (os_user, hostname) = Self::current_identity();
        let entry = LedgerEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            os_user,
            hostname,
            event,
            req_id: req_id.map(|s| s.to_string()),
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

    // rtmx:req REQ-AUDIT-001
    #[tokio::test]
    async fn ledger_creates_log_directory() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        assert!(!log_dir.exists());

        let _ledger = make_ledger(&log_dir).await;
        assert!(log_dir.exists());
    }

    // rtmx:req REQ-AUDIT-001
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

    // rtmx:req REQ-AUDIT-001
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

    // rtmx:req REQ-AUDIT-001
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

    // rtmx:req REQ-AUDIT-006
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

    // rtmx:req REQ-AUDIT-001
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

    // rtmx:req REQ-AUDIT-001
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

    // rtmx:req REQ-AUDIT-004
    #[tokio::test]
    async fn ledger_has_max_file_size_constant() {
        // Verify the rotation threshold is 10 MB
        assert_eq!(super::MAX_FILE_SIZE, 10 * 1024 * 1024);
    }

    // rtmx:req REQ-AUDIT-004
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

    // rtmx:req REQ-AUDIT-007
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

    // rtmx:req REQ-AUDIT-007
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

    // rtmx:req REQ-AUDIT-007
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

    // rtmx:req REQ-AUDIT-007
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

    // --- Crash recovery tests (REQ-AUDIT-008) ---

    /// Helper: write raw content to a dated log file in the given dir.
    fn write_raw_log(log_dir: &Path, content: &str) -> PathBuf {
        std::fs::create_dir_all(log_dir).unwrap();
        let date = chrono::Utc::now().format("%Y-%m-%d");
        let path = log_dir.join(format!("aegis-{date}.jsonl"));
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Helper: build a minimal valid JSONL line.
    fn valid_jsonl_line() -> String {
        serde_json::to_string(&serde_json::json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "os_user": "test",
            "hostname": "host",
            "event": "SessionStarted"
        }))
        .unwrap()
    }

    // rtmx:req REQ-AUDIT-008
    #[test]
    fn recover_clean_file_returns_zero_quarantined() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let line = valid_jsonl_line();
        write_raw_log(&log_dir, &format!("{line}\n{line}\n"));

        let report = JsonlLedger::recover(&log_dir);

        assert_eq!(report.quarantined_entries, 0);
        assert_eq!(report.valid_entries, 2);
        assert!(report.quarantine_path.is_none());
    }

    // rtmx:req REQ-AUDIT-008
    #[test]
    fn recover_truncated_last_line_quarantines_it() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let line = valid_jsonl_line();
        // Simulate crash: valid line followed by truncated JSON
        write_raw_log(&log_dir, &format!("{line}\n{{\"trunca"));

        let report = JsonlLedger::recover(&log_dir);

        assert_eq!(report.quarantined_entries, 1);
        assert_eq!(report.valid_entries, 2); // 1 original + 1 LEDGER_REPAIRED
        assert!(report.quarantine_path.is_some());
    }

    // rtmx:req REQ-AUDIT-008
    #[test]
    fn recover_preserves_all_valid_entries() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let line = valid_jsonl_line();
        let path = write_raw_log(&log_dir, &format!("{line}\n{line}\n{line}\n"));

        let report = JsonlLedger::recover(&log_dir);

        assert_eq!(report.valid_entries, 3);
        assert_eq!(report.quarantined_entries, 0);

        // File content unchanged (no repair entry added)
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 3);
    }

    // rtmx:req REQ-AUDIT-008
    #[test]
    fn quarantine_file_contains_corrupt_entries() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let line = valid_jsonl_line();
        write_raw_log(&log_dir, &format!("{line}\n{{\"broken\n{line}\n"));

        let report = JsonlLedger::recover(&log_dir);

        let qpath = report.quarantine_path.unwrap();
        assert!(qpath.exists());
        let corrupt_content = std::fs::read_to_string(&qpath).unwrap();
        assert!(corrupt_content.contains("{\"broken"));
        assert_eq!(
            corrupt_content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count(),
            1
        );
    }

    // rtmx:req REQ-AUDIT-008
    #[test]
    fn ledger_repaired_entry_appended_after_recovery() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let line = valid_jsonl_line();
        let path = write_raw_log(&log_dir, &format!("{line}\n{{\"bad\n"));

        JsonlLedger::recover(&log_dir);

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

        // Last line should be the LEDGER_REPAIRED entry
        let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        assert_eq!(last["event"], "LEDGER_REPAIRED");
        assert_eq!(last["quarantined_entries"], 1);
    }

    // rtmx:req REQ-AUDIT-008
    #[test]
    fn recover_empty_file_returns_zero_quarantined() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        write_raw_log(&log_dir, "");

        let report = JsonlLedger::recover(&log_dir);

        assert_eq!(report.quarantined_entries, 0);
        assert_eq!(report.valid_entries, 0);
        assert!(report.quarantine_path.is_none());
    }

    // rtmx:req REQ-AUDIT-008
    #[test]
    fn recover_multiple_corrupt_lines_all_quarantined() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        let line = valid_jsonl_line();
        write_raw_log(&log_dir, &format!("{line}\n{{\"bad1\n{{\"bad2\n{{\"bad3\n"));

        let report = JsonlLedger::recover(&log_dir);

        assert_eq!(report.quarantined_entries, 3);
        // 1 original valid + 1 LEDGER_REPAIRED
        assert_eq!(report.valid_entries, 2);

        let qpath = report.quarantine_path.unwrap();
        let corrupt_content = std::fs::read_to_string(&qpath).unwrap();
        let corrupt_lines: Vec<&str> = corrupt_content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        assert_eq!(corrupt_lines.len(), 3);
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

    // rtmx:req REQ-AUDIT-003
    #[tokio::test]
    async fn record_with_req_includes_req_id_in_entry() {
        let dir = TempDir::new().unwrap();
        let ledger = make_ledger(dir.path()).await;

        let event = session_started_event();
        ledger
            .record_with_req(&event, Some("REQ-BUILD-001"))
            .await
            .unwrap();

        let entries = read_log_entries(dir.path());
        assert_eq!(entries.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(&entries[0]).unwrap();
        assert_eq!(
            parsed["req_id"].as_str(),
            Some("REQ-BUILD-001"),
            "entry should contain req_id field"
        );
    }

    // rtmx:req REQ-AUDIT-003
    #[tokio::test]
    async fn record_without_req_omits_req_id() {
        let dir = TempDir::new().unwrap();
        let ledger = make_ledger(dir.path()).await;

        let event = session_started_event();
        ledger.record(&event).await.unwrap();

        let entries = read_log_entries(dir.path());
        assert_eq!(entries.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(&entries[0]).unwrap();
        assert!(
            parsed.get("req_id").is_none() || parsed["req_id"].is_null(),
            "entry should not contain req_id when not provided"
        );
    }

    // rtmx:req REQ-HITL-016
    #[tokio::test]
    async fn test_kill_switch_audit_event_recorded() {
        let dir = TempDir::new().unwrap();
        let ledger = make_ledger(dir.path()).await;

        let session_id = SessionId::new();
        let timestamp = Utc::now();
        let pending_tool_count = 3;

        let event = DomainEvent::KillSwitch {
            session_id: session_id.clone(),
            timestamp,
            pending_tool_count,
        };

        ledger.record(&event).await.unwrap();

        let entries = read_log_entries(dir.path());
        assert_eq!(entries.len(), 1, "Kill switch event must be recorded");

        let parsed: serde_json::Value = serde_json::from_str(&entries[0]).unwrap();

        // Verify mandatory audit fields
        assert!(parsed.get("timestamp").is_some(), "Missing timestamp");
        assert!(parsed.get("os_user").is_some(), "Missing os_user");
        assert!(parsed.get("hostname").is_some(), "Missing hostname");

        // Verify the event payload
        let event_val = &parsed["event"];
        assert_eq!(event_val["KillSwitch"]["pending_tool_count"], 3);
        assert!(
            event_val["KillSwitch"]["session_id"].is_string(),
            "session_id must be present in KillSwitch event"
        );
        assert!(
            event_val["KillSwitch"]["timestamp"].is_string(),
            "timestamp must be present in KillSwitch event"
        );
    }

    // rtmx:req REQ-SECURITY-024
    #[tokio::test]
    async fn test_cui_blocked_audit_event() {
        let dir = TempDir::new().unwrap();
        let ledger = make_ledger(dir.path()).await;

        let session_id = SessionId::new();
        let event = DomainEvent::CuiBlocked {
            session_id: session_id.clone(),
            endpoint_url: "https://api.openai.com/v1".to_string(),
            pattern_matched: "CUI_BANNER".to_string(),
            timestamp: Utc::now(),
        };

        ledger.record(&event).await.unwrap();

        let entries = read_log_entries(dir.path());
        assert_eq!(entries.len(), 1, "CuiBlocked event must be recorded");

        let parsed: serde_json::Value = serde_json::from_str(&entries[0]).unwrap();

        // Verify mandatory audit fields
        assert!(parsed.get("timestamp").is_some(), "Missing timestamp");
        assert!(parsed.get("os_user").is_some(), "Missing os_user");
        assert!(parsed.get("hostname").is_some(), "Missing hostname");

        // Verify event payload
        let event_val = &parsed["event"];
        assert!(
            event_val["CuiBlocked"]["session_id"].is_string(),
            "session_id must be present in CuiBlocked event"
        );
        assert_eq!(
            event_val["CuiBlocked"]["endpoint_url"].as_str(),
            Some("https://api.openai.com/v1"),
            "endpoint_url must match"
        );
        assert_eq!(
            event_val["CuiBlocked"]["pattern_matched"].as_str(),
            Some("CUI_BANNER"),
            "pattern_matched must match"
        );
        assert!(
            event_val["CuiBlocked"]["timestamp"].is_string(),
            "timestamp must be present in CuiBlocked event"
        );
    }

    // rtmx:req REQ-AUDIT-003
    #[tokio::test]
    async fn record_with_req_none_omits_req_id() {
        let dir = TempDir::new().unwrap();
        let ledger = make_ledger(dir.path()).await;

        let event = session_started_event();
        ledger.record_with_req(&event, None).await.unwrap();

        let entries = read_log_entries(dir.path());
        let parsed: serde_json::Value = serde_json::from_str(&entries[0]).unwrap();
        assert!(
            parsed.get("req_id").is_none() || parsed["req_id"].is_null(),
            "entry should not contain req_id when None"
        );
    }
}
