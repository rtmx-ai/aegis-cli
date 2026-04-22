//! JSONL scanner for TokensConsumed audit events (REQ-AUDIT-021a).
//!
//! Reads JSONL audit ledger files and extracts `TokensConsumed` events
//! for cost reporting. Malformed lines are skipped with a warning.

use aegis_domain::error::DomainError;
use std::path::Path;

/// A parsed TokensConsumed record from the audit ledger.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TokensConsumedRecord {
    pub session_id: String,
    pub provider_kind: String,
    pub model: String,
    pub project_id: Option<String>,
    pub region: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub timestamp: String,
}

/// Scan all `*.jsonl` files in `logs_dir` and return every
/// `TokensConsumed` record found, sorted chronologically by timestamp.
pub fn scan_ledger_files(logs_dir: &Path) -> Result<Vec<TokensConsumedRecord>, DomainError> {
    let entries = std::fs::read_dir(logs_dir).map_err(|e| DomainError::AuditError {
        message: format!("Failed to read log directory {}: {e}", logs_dir.display()),
    })?;

    let mut records = Vec::new();

    for dir_entry in entries.flatten() {
        let path = dir_entry.path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            match scan_single_file(&path) {
                Ok(mut file_records) => records.append(&mut file_records),
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Skipping ledger file due to error"
                    );
                }
            }
        }
    }

    records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(records)
}

/// Scan a single JSONL file and return all `TokensConsumed` records.
pub fn scan_single_file(path: &Path) -> Result<Vec<TokensConsumedRecord>, DomainError> {
    let content = std::fs::read_to_string(path).map_err(|e| DomainError::AuditError {
        message: format!("Failed to read file {}: {e}", path.display()),
    })?;

    let mut records = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse the line as a generic JSON value first.
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    line = line_num + 1,
                    error = %e,
                    "Skipping malformed JSONL line"
                );
                continue;
            }
        };

        // Ledger entries wrap the DomainEvent in an "event" field.
        // The DomainEvent is externally tagged, so TokensConsumed appears
        // as: {"event": {"TokensConsumed": { ... }}}
        let event_value = match value.get("event") {
            Some(v) => v,
            None => continue,
        };

        let tc_fields = match event_value.get("TokensConsumed") {
            Some(v) => v,
            None => continue,
        };

        match serde_json::from_value::<TokensConsumedRecord>(tc_fields.clone()) {
            Ok(record) => records.push(record),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    line = line_num + 1,
                    error = %e,
                    "Skipping malformed TokensConsumed entry"
                );
            }
        }
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::event::DomainEvent;
    use tempfile::TempDir;

    /// Helper: serialize a DomainEvent inside a ledger entry envelope.
    fn ledger_line(event: &DomainEvent) -> String {
        serde_json::to_string(&serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "os_user": "testuser",
            "hostname": "testhost",
            "event": event,
        }))
        .unwrap()
    }

    fn make_tokens_consumed(ts: &str) -> DomainEvent {
        DomainEvent::TokensConsumed {
            session_id: "sess-001".to_string(),
            provider_kind: "vertex".to_string(),
            model: "gemini-2.5-pro".to_string(),
            project_id: Some("my-project".to_string()),
            region: Some("us-central1".to_string()),
            input_tokens: 1000,
            output_tokens: 500,
            timestamp: ts.to_string(),
        }
    }

    fn make_session_started() -> DomainEvent {
        use aegis_domain::types::SessionId;
        DomainEvent::SessionStarted {
            session_id: SessionId::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    // rtmx:req REQ-AUDIT-021a
    #[test]
    fn test_scan_parses_tokens_consumed_from_jsonl() {
        let tmp = TempDir::new().unwrap();
        let event = make_tokens_consumed("2026-04-18T12:00:00Z");
        let line = ledger_line(&event);
        std::fs::write(tmp.path().join("test.jsonl"), format!("{line}\n")).unwrap();

        let records = scan_ledger_files(tmp.path()).unwrap();
        assert_eq!(records.len(), 1);

        let r = &records[0];
        assert_eq!(r.session_id, "sess-001");
        assert_eq!(r.provider_kind, "vertex");
        assert_eq!(r.model, "gemini-2.5-pro");
        assert_eq!(r.project_id.as_deref(), Some("my-project"));
        assert_eq!(r.region.as_deref(), Some("us-central1"));
        assert_eq!(r.input_tokens, 1000);
        assert_eq!(r.output_tokens, 500);
        assert_eq!(r.timestamp, "2026-04-18T12:00:00Z");
    }

    // rtmx:req REQ-AUDIT-021a
    #[test]
    fn test_scan_skips_non_tokens_consumed_events() {
        let tmp = TempDir::new().unwrap();
        let tc = ledger_line(&make_tokens_consumed("2026-04-18T12:00:00Z"));
        let ss = ledger_line(&make_session_started());
        std::fs::write(tmp.path().join("test.jsonl"), format!("{ss}\n{tc}\n")).unwrap();

        let records = scan_ledger_files(tmp.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].provider_kind, "vertex");
    }

    // rtmx:req REQ-AUDIT-021a
    #[test]
    fn test_scan_skips_malformed_lines() {
        let tmp = TempDir::new().unwrap();
        let tc = ledger_line(&make_tokens_consumed("2026-04-18T12:00:00Z"));
        let content = format!("{{not valid json\n{tc}\n");
        std::fs::write(tmp.path().join("test.jsonl"), content).unwrap();

        let records = scan_ledger_files(tmp.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "sess-001");
    }

    // rtmx:req REQ-AUDIT-021a
    #[test]
    fn test_scan_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let records = scan_ledger_files(tmp.path()).unwrap();
        assert!(records.is_empty());
    }

    // rtmx:req REQ-AUDIT-021a
    #[test]
    fn test_scan_missing_directory() {
        let result = scan_ledger_files(Path::new("/nonexistent/dir/12345"));
        assert!(result.is_err());
    }

    // rtmx:req REQ-AUDIT-021a
    #[test]
    fn test_scan_returns_chronological_order() {
        let tmp = TempDir::new().unwrap();
        let late = ledger_line(&make_tokens_consumed("2026-04-19T12:00:00Z"));
        let early = ledger_line(&make_tokens_consumed("2026-04-17T12:00:00Z"));
        let mid = ledger_line(&make_tokens_consumed("2026-04-18T12:00:00Z"));
        std::fs::write(
            tmp.path().join("test.jsonl"),
            format!("{late}\n{early}\n{mid}\n"),
        )
        .unwrap();

        let records = scan_ledger_files(tmp.path()).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].timestamp, "2026-04-17T12:00:00Z");
        assert_eq!(records[1].timestamp, "2026-04-18T12:00:00Z");
        assert_eq!(records[2].timestamp, "2026-04-19T12:00:00Z");
    }

    // rtmx:req REQ-AUDIT-021a
    #[test]
    fn test_scan_multiple_files() {
        let tmp = TempDir::new().unwrap();
        let tc1 = ledger_line(&make_tokens_consumed("2026-04-18T12:00:00Z"));
        let tc2 = ledger_line(&make_tokens_consumed("2026-04-19T12:00:00Z"));
        std::fs::write(tmp.path().join("a.jsonl"), format!("{tc1}\n")).unwrap();
        std::fs::write(tmp.path().join("b.jsonl"), format!("{tc2}\n")).unwrap();

        let records = scan_ledger_files(tmp.path()).unwrap();
        assert_eq!(records.len(), 2);
        // Should be chronologically sorted across files
        assert_eq!(records[0].timestamp, "2026-04-18T12:00:00Z");
        assert_eq!(records[1].timestamp, "2026-04-19T12:00:00Z");
    }
}
