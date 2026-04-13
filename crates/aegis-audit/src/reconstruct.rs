//! Session reconstruction from audit ledger entries (REQ-AUDIT-017).
//!
//! Rebuilds a complete session timeline from ledger entries, including
//! event counts and time bounds.

use crate::search::{self, SearchQuery};
use std::io;
use std::path::Path;

/// A reconstructed session timeline with aggregated statistics.
#[derive(Debug)]
pub struct SessionTimeline {
    /// The session ID that was reconstructed.
    pub session_id: String,
    /// All ledger entries for this session, in scan order.
    pub events: Vec<serde_json::Value>,
    /// Earliest timestamp found in the session.
    pub started: Option<chrono::DateTime<chrono::Utc>>,
    /// Latest timestamp found in the session.
    pub ended: Option<chrono::DateTime<chrono::Utc>>,
    /// Number of ToolCallExecuted events.
    pub tool_calls: usize,
    /// Number of approved decisions.
    pub approvals: usize,
    /// Number of denied decisions.
    pub denials: usize,
}

/// Reconstruct a session timeline from ledger entries.
///
/// Searches all segments for entries matching the given `session_id`,
/// then aggregates timestamps and event counts into a
/// [`SessionTimeline`].
pub fn reconstruct_session(log_dir: &Path, session_id: &str) -> io::Result<SessionTimeline> {
    let query = SearchQuery {
        session_id: Some(session_id.to_string()),
        ..Default::default()
    };

    let result = search::search_ledger(log_dir, &query)?;

    let mut timeline = SessionTimeline {
        session_id: session_id.to_string(),
        events: Vec::new(),
        started: None,
        ended: None,
        tool_calls: 0,
        approvals: 0,
        denials: 0,
    };

    for entry in result.entries {
        // Parse timestamp for time bounds.
        if let Some(ts_str) = entry.get("timestamp").and_then(|v| v.as_str())
            && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str)
        {
            let dt = dt.with_timezone(&chrono::Utc);
            timeline.started = Some(match timeline.started {
                Some(existing) if existing < dt => existing,
                _ => dt,
            });
            timeline.ended = Some(match timeline.ended {
                Some(existing) if existing > dt => existing,
                _ => dt,
            });
        }

        // Count event types.
        if let Some(event) = entry.get("event")
            && let Some(obj) = event.as_object()
        {
            for key in obj.keys() {
                match key.as_str() {
                    "ToolCallExecuted" => {
                        timeline.tool_calls += 1;
                    }
                    "ToolCallApproved" => {
                        // Check decision field.
                        if let Some(variant) = obj.get(key) {
                            let decision = variant.get("decision").and_then(|d| d.as_str());
                            match decision {
                                Some("Denied") => {
                                    timeline.denials += 1;
                                }
                                _ => {
                                    timeline.approvals += 1;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        timeline.events.push(entry);
    }

    Ok(timeline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_segment(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let content = lines.join("\n") + "\n";
        std::fs::write(&path, content).unwrap();
        path
    }

    fn make_entry(
        event_type: &str,
        session_id: &str,
        ts: &str,
        extra: Option<serde_json::Value>,
    ) -> String {
        let mut variant = serde_json::json!({
            "session_id": session_id,
            "timestamp": ts,
        });
        if let Some(extra) = extra
            && let (Some(v), Some(e)) = (variant.as_object_mut(), extra.as_object())
        {
            for (k, val) in e {
                v.insert(k.clone(), val.clone());
            }
        }
        serde_json::to_string(&serde_json::json!({
            "timestamp": ts,
            "os_user": "testuser",
            "hostname": "testhost",
            "event": { event_type: variant }
        }))
        .unwrap()
    }

    // rtmx:req REQ-AUDIT-017
    #[test]
    fn reconstruct_empty_session() {
        let tmp = TempDir::new().unwrap();
        // Write a segment with a different session.
        let e1 = make_entry(
            "SessionStarted",
            "other-session",
            "2026-04-10T00:00:00Z",
            None,
        );
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1]);

        let timeline = reconstruct_session(tmp.path(), "nonexistent").unwrap();

        assert_eq!(timeline.session_id, "nonexistent");
        assert!(timeline.events.is_empty());
        assert!(timeline.started.is_none());
        assert!(timeline.ended.is_none());
        assert_eq!(timeline.tool_calls, 0);
    }

    // rtmx:req REQ-AUDIT-017
    #[test]
    fn reconstruct_counts_tool_calls() {
        let tmp = TempDir::new().unwrap();
        let sid = "session-tools";
        let e1 = make_entry("SessionStarted", sid, "2026-04-10T00:00:00Z", None);
        let e2 = make_entry("ToolCallExecuted", sid, "2026-04-10T00:01:00Z", None);
        let e3 = make_entry("ToolCallExecuted", sid, "2026-04-10T00:02:00Z", None);
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2, &e3]);

        let timeline = reconstruct_session(tmp.path(), sid).unwrap();

        assert_eq!(timeline.tool_calls, 2);
        assert_eq!(timeline.events.len(), 3);
    }

    // rtmx:req REQ-AUDIT-017
    #[test]
    fn reconstruct_counts_approvals_and_denials() {
        let tmp = TempDir::new().unwrap();
        let sid = "session-decisions";
        let e1 = make_entry(
            "ToolCallApproved",
            sid,
            "2026-04-10T00:01:00Z",
            Some(serde_json::json!({"decision": "Approved"})),
        );
        let e2 = make_entry(
            "ToolCallApproved",
            sid,
            "2026-04-10T00:02:00Z",
            Some(serde_json::json!({"decision": "Denied"})),
        );
        let e3 = make_entry(
            "ToolCallApproved",
            sid,
            "2026-04-10T00:03:00Z",
            Some(serde_json::json!({"decision": "Approved"})),
        );
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2, &e3]);

        let timeline = reconstruct_session(tmp.path(), sid).unwrap();

        assert_eq!(timeline.approvals, 2);
        assert_eq!(timeline.denials, 1);
    }

    // rtmx:req REQ-AUDIT-017
    #[test]
    fn reconstruct_tracks_start_end() {
        let tmp = TempDir::new().unwrap();
        let sid = "session-time";
        let e1 = make_entry("SessionStarted", sid, "2026-04-10T08:00:00Z", None);
        let e2 = make_entry("ToolCallExecuted", sid, "2026-04-10T09:30:00Z", None);
        let e3 = make_entry("SessionEnded", sid, "2026-04-10T10:00:00Z", None);
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&e1, &e2, &e3]);

        let timeline = reconstruct_session(tmp.path(), sid).unwrap();

        let started = timeline.started.unwrap();
        let ended = timeline.ended.unwrap();

        assert_eq!(started.to_rfc3339(), "2026-04-10T08:00:00+00:00");
        assert_eq!(ended.to_rfc3339(), "2026-04-10T10:00:00+00:00");
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn reconstruct_nonexistent_log_dir() {
        let result =
            reconstruct_session(Path::new("/tmp/aegis-nonexistent-dir-99999"), "any-session");

        assert!(
            result.is_err(),
            "nonexistent log dir should return an error"
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn reconstruct_with_malformed_entries() {
        let tmp = TempDir::new().unwrap();
        let sid = "session-malformed";
        let valid = make_entry("SessionStarted", sid, "2026-04-10T00:00:00Z", None);
        // Entry missing session_id -- will not match the session filter.
        let malformed = serde_json::to_string(&serde_json::json!({
            "timestamp": "2026-04-10T00:01:00Z",
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {"ToolCallExecuted": {"tool": "ls"}}
        }))
        .unwrap();
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&valid, &malformed]);

        let timeline = reconstruct_session(tmp.path(), sid).unwrap();

        assert_eq!(
            timeline.events.len(),
            1,
            "only the entry with matching session_id should appear"
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn reconstruct_with_malformed_timestamps() {
        let tmp = TempDir::new().unwrap();
        let sid = "session-badts";
        // Entry with an invalid timestamp string.
        let bad_ts = serde_json::to_string(&serde_json::json!({
            "timestamp": "not-a-timestamp",
            "os_user": "testuser",
            "hostname": "testhost",
            "session_id": sid,
            "event": {"SessionStarted": {"session_id": sid, "timestamp": "not-a-timestamp"}}
        }))
        .unwrap();
        let good = make_entry("ToolCallExecuted", sid, "2026-04-10T05:00:00Z", None);
        write_segment(tmp.path(), "aegis-2026-04-10.jsonl", &[&bad_ts, &good]);

        let timeline = reconstruct_session(tmp.path(), sid).unwrap();

        // The bad-timestamp entry is still included (search matches by session_id),
        // but started/ended should reflect only the parseable timestamp.
        assert_eq!(timeline.events.len(), 2);
        assert!(
            timeline.started.is_some(),
            "should have a start time from the valid entry"
        );
        assert_eq!(
            timeline.started.unwrap().to_rfc3339(),
            "2026-04-10T05:00:00+00:00"
        );
    }

    // rtmx:req REQ-TEST-009
    #[test]
    fn reconstruct_counts_are_accurate_with_mixed_events() {
        let tmp = TempDir::new().unwrap();
        let sid = "session-mixed";
        let e1 = make_entry("SessionStarted", sid, "2026-04-10T00:00:00Z", None);
        let e2 = make_entry("ToolCallExecuted", sid, "2026-04-10T00:01:00Z", None);
        let e3 = make_entry("ToolCallExecuted", sid, "2026-04-10T00:02:00Z", None);
        let e4 = make_entry(
            "ToolCallApproved",
            sid,
            "2026-04-10T00:03:00Z",
            Some(serde_json::json!({"decision": "Approved"})),
        );
        let e5 = make_entry(
            "ToolCallApproved",
            sid,
            "2026-04-10T00:04:00Z",
            Some(serde_json::json!({"decision": "Denied"})),
        );
        let e6 = make_entry("SessionEnded", sid, "2026-04-10T00:05:00Z", None);
        write_segment(
            tmp.path(),
            "aegis-2026-04-10.jsonl",
            &[&e1, &e2, &e3, &e4, &e5, &e6],
        );

        let timeline = reconstruct_session(tmp.path(), sid).unwrap();

        assert_eq!(timeline.events.len(), 6, "all events should be present");
        assert_eq!(timeline.tool_calls, 2, "two ToolCallExecuted events");
        assert_eq!(timeline.approvals, 1, "one Approved decision");
        assert_eq!(timeline.denials, 1, "one Denied decision");
    }
}
