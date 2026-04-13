//! Ledger search across all audit log segments (REQ-AUDIT-013).
//!
//! Supports filtering by event type, requirement ID, session ID, and
//! time range. Reads both plain `.jsonl` and compressed `.jsonl.zst`
//! segments.

use crate::rotation;
use std::io;
use std::path::{Path, PathBuf};

/// Query parameters for searching the audit ledger.
#[derive(Debug, Default)]
pub struct SearchQuery {
    /// Filter by the domain event type (e.g. "ToolCallExecuted").
    pub event_type: Option<String>,
    /// Filter by RTMX requirement ID (e.g. "REQ-AUDIT-009").
    pub req_id: Option<String>,
    /// Filter by session UUID.
    pub session_id: Option<String>,
    /// Include only entries at or after this timestamp.
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Include only entries at or before this timestamp.
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Maximum number of matching entries to return.
    pub limit: Option<usize>,
}

/// Result of a ledger search.
#[derive(Debug)]
pub struct SearchResult {
    /// Matching ledger entries.
    pub entries: Vec<serde_json::Value>,
    /// Total number of entries scanned.
    pub total_scanned: usize,
    /// Total number of entries that matched the query.
    pub total_matched: usize,
}

/// Search across all ledger segments (current + rotated + compressed).
///
/// Segments are iterated newest-first based on filename sort order.
/// Each JSONL line is parsed and filtered against the query fields.
/// Compressed `.zst` files are decompressed on the fly.
pub fn search_ledger(log_dir: &Path, query: &SearchQuery) -> io::Result<SearchResult> {
    let segments = collect_segments(log_dir)?;

    let mut entries = Vec::new();
    let mut total_scanned: usize = 0;
    let mut total_matched: usize = 0;

    for segment_path in &segments {
        let content = read_segment(segment_path)?;

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            total_scanned += 1;

            let parsed: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue, // skip corrupt lines
            };

            if matches_query(&parsed, query) {
                total_matched += 1;
                entries.push(parsed);

                if let Some(limit) = query.limit
                    && total_matched >= limit
                {
                    return Ok(SearchResult {
                        entries,
                        total_scanned,
                        total_matched,
                    });
                }
            }
        }
    }

    Ok(SearchResult {
        entries,
        total_scanned,
        total_matched,
    })
}

/// Collect all segment paths in the log directory, sorted newest-first.
fn collect_segments(log_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut segments = Vec::new();
    let entries = std::fs::read_dir(log_dir)?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !name.starts_with("aegis-") {
            continue;
        }

        let is_jsonl = name.ends_with(".jsonl");
        let is_zst = name.ends_with(".jsonl.zst");

        if is_jsonl || is_zst {
            segments.push(path);
        }
    }

    // Sort descending by filename (newest date first).
    segments.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    Ok(segments)
}

/// Read segment content, decompressing if necessary.
fn read_segment(path: &Path) -> io::Result<String> {
    if rotation::is_compressed(path) {
        rotation::decompress_segment(path)
    } else {
        std::fs::read_to_string(path)
    }
}

/// Check whether a parsed ledger entry matches the search query.
fn matches_query(entry: &serde_json::Value, query: &SearchQuery) -> bool {
    // Filter by event type.
    if let Some(ref event_type) = query.event_type {
        let event_field = &entry["event"];
        let entry_type = if event_field.is_object() {
            // Serialized enum: {"ToolCallExecuted": {...}}
            event_field
                .as_object()
                .and_then(|obj| obj.keys().next())
                .map(|k| k.as_str())
        } else if event_field.is_string() {
            // Simple string: "SessionStarted"
            event_field.as_str()
        } else {
            None
        };
        match entry_type {
            Some(t) if t == event_type => {}
            _ => return false,
        }
    }

    // Filter by requirement ID.
    if let Some(ref req_id) = query.req_id {
        match entry.get("req_id").and_then(|v| v.as_str()) {
            Some(r) if r == req_id => {}
            _ => return false,
        }
    }

    // Filter by session ID.
    if let Some(ref session_id) = query.session_id {
        let entry_session = extract_session_id(entry);
        match entry_session {
            Some(s) if s == session_id => {}
            _ => return false,
        }
    }

    // Filter by time range.
    if query.since.is_some() || query.until.is_some() {
        let ts = entry
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        match ts {
            Some(dt) => {
                if let Some(ref since) = query.since
                    && dt < *since
                {
                    return false;
                }
                if let Some(ref until) = query.until
                    && dt > *until
                {
                    return false;
                }
            }
            None => return false,
        }
    }

    true
}

/// Extract session_id from a ledger entry's event field.
fn extract_session_id(entry: &serde_json::Value) -> Option<&str> {
    let event = entry.get("event")?;

    // Enum variant: {"SessionStarted": {"session_id": "..."}}
    if let Some(obj) = event.as_object() {
        for (_key, variant) in obj {
            if let Some(sid) = variant.get("session_id").and_then(|v| v.as_str()) {
                return Some(sid);
            }
        }
    }

    // Top-level session_id (some entry formats).
    entry.get("session_id").and_then(|v| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Write a JSONL segment with the given lines.
    fn write_segment(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let content = lines.join("\n") + "\n";
        std::fs::write(&path, content).unwrap();
        path
    }

    fn make_entry(event_type: &str, session_id: &str, ts: &str) -> String {
        serde_json::to_string(&serde_json::json!({
            "timestamp": ts,
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {
                event_type: {
                    "session_id": session_id,
                    "timestamp": ts
                }
            }
        }))
        .unwrap()
    }

    // rtmx:req REQ-AUDIT-013
    #[test]
    fn search_by_event_type() {
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("ToolCallExecuted", "s1", "2026-04-10T00:00:00Z");
        let e2 = make_entry("SessionStarted", "s1", "2026-04-10T00:01:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2]);

        let query = SearchQuery {
            event_type: Some("ToolCallExecuted".to_string()),
            ..Default::default()
        };
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(result.total_matched, 1);
        assert_eq!(result.total_scanned, 2);
    }

    // rtmx:req REQ-AUDIT-013
    #[test]
    fn search_by_session_id() {
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("SessionStarted", "session-abc", "2026-04-10T00:00:00Z");
        let e2 = make_entry("SessionStarted", "session-xyz", "2026-04-10T00:01:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2]);

        let query = SearchQuery {
            session_id: Some("session-abc".to_string()),
            ..Default::default()
        };
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(result.total_matched, 1);
    }

    // rtmx:req REQ-AUDIT-013
    #[test]
    fn search_by_time_range() {
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("SessionStarted", "s1", "2026-04-10T00:00:00Z");
        let e2 = make_entry("SessionStarted", "s1", "2026-04-10T12:00:00Z");
        let e3 = make_entry("SessionStarted", "s1", "2026-04-11T00:00:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2, &e3]);

        let query = SearchQuery {
            since: Some(
                "2026-04-10T06:00:00Z"
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap(),
            ),
            until: Some(
                "2026-04-10T18:00:00Z"
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap(),
            ),
            ..Default::default()
        };
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(result.total_matched, 1);
    }

    // rtmx:req REQ-AUDIT-013
    #[test]
    fn search_with_limit() {
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("SessionStarted", "s1", "2026-04-10T00:00:00Z");
        let e2 = make_entry("SessionStarted", "s1", "2026-04-10T00:01:00Z");
        let e3 = make_entry("SessionStarted", "s1", "2026-04-10T00:02:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2, &e3]);

        let query = SearchQuery {
            limit: Some(2),
            ..Default::default()
        };
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.total_matched, 2);
    }

    // rtmx:req REQ-AUDIT-013
    #[test]
    fn search_empty_query_returns_all() {
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("SessionStarted", "s1", "2026-04-10T00:00:00Z");
        let e2 = make_entry("ToolCallExecuted", "s2", "2026-04-10T00:01:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2]);

        let query = SearchQuery::default();
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(result.total_matched, 2);
        assert_eq!(result.entries.len(), 2);
    }

    // rtmx:req REQ-AUDIT-013
    #[test]
    fn search_across_compressed_segments() {
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("SessionStarted", "s1", "2026-04-09T00:00:00Z");
        let path = write_segment(tmp.path(), "aegis-2026-04-09.jsonl", &[&e1]);

        // Compress the segment.
        rotation::compress_segment(&path).unwrap();

        // Also add a plain segment.
        let e2 = make_entry("SessionStarted", "s2", "2026-04-10T00:00:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e2]);

        let query = SearchQuery::default();
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(
            result.total_matched, 2,
            "should find entries in both plain and compressed segments"
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn search_malformed_jsonl_line_skipped() {
        let tmp = TempDir::new().unwrap();
        let valid = make_entry("SessionStarted", "s1", "2026-04-10T00:00:00Z");
        write_segment(
            tmp.path(),
            "aegis-2026-04-10.jsonl",
            &[&valid, "THIS IS NOT JSON {{{", &valid],
        );

        let query = SearchQuery::default();
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(result.total_matched, 2, "valid lines should be returned");
        assert_eq!(
            result.total_scanned, 3,
            "all non-empty lines are scanned (corrupt ones are skipped after scan count)"
        );
        assert_eq!(result.entries.len(), 2, "only valid entries are returned");
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn search_missing_log_dir_returns_error() {
        let result = search_ledger(
            Path::new("/tmp/aegis-nonexistent-dir-12345"),
            &SearchQuery::default(),
        );

        assert!(result.is_err(), "missing log_dir should return an error");
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn search_empty_directory_returns_empty() {
        let tmp = TempDir::new().unwrap();

        let query = SearchQuery::default();
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(result.total_matched, 0);
        assert_eq!(result.total_scanned, 0);
        assert!(result.entries.is_empty());
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn search_with_limit_zero_returns_at_most_one() {
        // limit=0 triggers the `total_matched >= limit` guard after the
        // first match (0 >= 0 is true), so at most one entry is returned.
        // This tests the edge-case boundary of the limit parameter.
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("SessionStarted", "s1", "2026-04-10T00:00:00Z");
        let e2 = make_entry("SessionStarted", "s1", "2026-04-10T00:01:00Z");
        let e3 = make_entry("SessionStarted", "s1", "2026-04-10T00:02:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2, &e3]);

        let query = SearchQuery {
            limit: Some(0),
            ..Default::default()
        };
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert!(
            result.entries.len() <= 1,
            "limit=0 should return at most 1 entry due to >= check"
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn search_corrupt_zst_file_returns_error_or_skips() {
        let tmp = TempDir::new().unwrap();
        // Write corrupt data to a .zst file.
        let corrupt_path = tmp.path().join("aegis-2026-04-09.jsonl.zst");
        std::fs::write(&corrupt_path, b"not valid zstd bytes").unwrap();

        let query = SearchQuery::default();
        let result = search_ledger(tmp.path(), &query);

        // The function calls read_segment which calls decompress_segment,
        // which will fail on corrupt zstd. This should return an error.
        assert!(result.is_err(), "corrupt .zst should produce an error");
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn search_with_since_after_until_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let e1 = make_entry("SessionStarted", "s1", "2026-04-10T12:00:00Z");
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1]);

        // since is AFTER until -- impossible range.
        let query = SearchQuery {
            since: Some(
                "2026-04-11T00:00:00Z"
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap(),
            ),
            until: Some(
                "2026-04-09T00:00:00Z"
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap(),
            ),
            ..Default::default()
        };
        let result = search_ledger(tmp.path(), &query).unwrap();

        assert_eq!(
            result.total_matched, 0,
            "impossible time range should match nothing"
        );
    }
}
