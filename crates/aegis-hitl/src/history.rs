//! Approval history review (REQ-HITL-006).
//!
//! Extracts structured HITL approval/denial history from audit ledger
//! entries. The caller (typically aegis-cli) is responsible for reading
//! the ledger via `aegis_audit::search`; this module transforms raw
//! JSON entries into typed `HistoryEntry` values and formats them for
//! display.

use chrono::{DateTime, Utc};

/// HITL event types stored in the audit ledger.
pub const HITL_APPROVED: &str = "HitlApproved";
pub const HITL_DENIED: &str = "HitlDenied";
pub const HITL_SKIPPED: &str = "HitlSkipped";

/// All HITL event types that constitute approval history.
pub const HITL_EVENT_TYPES: &[&str] = &[HITL_APPROVED, HITL_DENIED, HITL_SKIPPED];

/// Query parameters for filtering approval history.
#[derive(Debug, Default)]
pub struct HistoryQuery {
    /// Filter to a specific session.
    pub session_id: Option<String>,
    /// Show only denied entries.
    pub denied_only: bool,
}

/// The decision recorded for a HITL gate interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Denied,
    Skipped,
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Decision::Approved => write!(f, "APPROVED"),
            Decision::Denied => write!(f, "DENIED"),
            Decision::Skipped => write!(f, "SKIPPED"),
        }
    }
}

/// A single approval history entry extracted from the audit ledger.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// When the decision was made.
    pub timestamp: DateTime<Utc>,
    /// Session in which the decision occurred.
    pub session_id: String,
    /// Name of the tool that was gated.
    pub tool_name: String,
    /// The decision (approved, denied, skipped).
    pub decision: Decision,
    /// Target path of the tool call, if applicable.
    pub target_path: String,
}

/// Extract structured history entries from raw audit ledger JSON values.
///
/// The caller should pre-filter to HITL event types using
/// `aegis_audit::search` with the appropriate `event_type` filter.
/// This function parses the event payload and applies the `HistoryQuery`
/// filters (session, denied-only).
pub fn extract_history(entries: &[serde_json::Value], query: &HistoryQuery) -> Vec<HistoryEntry> {
    let mut result = Vec::new();

    for entry in entries {
        if let Some(he) = parse_history_entry(entry) {
            // Apply session filter.
            if let Some(ref sid) = query.session_id
                && he.session_id != *sid
            {
                continue;
            }
            // Apply denied-only filter.
            if query.denied_only && he.decision != Decision::Denied {
                continue;
            }
            result.push(he);
        }
    }

    result
}

/// Parse a single audit ledger JSON value into a `HistoryEntry`.
///
/// Returns `None` if the entry does not match the expected HITL event
/// structure.
fn parse_history_entry(entry: &serde_json::Value) -> Option<HistoryEntry> {
    let timestamp = entry
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))?;

    let event = entry.get("event")?;
    let obj = event.as_object()?;

    // The event is a serialized enum: {"HitlApproved": {...}}
    let (event_type, variant) = obj.iter().next()?;

    let decision = match event_type.as_str() {
        "HitlApproved" => Decision::Approved,
        "HitlDenied" => Decision::Denied,
        "HitlSkipped" => Decision::Skipped,
        _ => return None,
    };

    let session_id = variant
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let tool_name = variant
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let target_path = variant
        .get("target_path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(HistoryEntry {
        timestamp,
        session_id,
        tool_name,
        decision,
        target_path,
    })
}

/// Format history entries for terminal display.
///
/// Produces a human-readable table with columns for timestamp, session,
/// decision, tool, and target path.
pub fn format_history(entries: &[HistoryEntry]) -> String {
    if entries.is_empty() {
        return "No HITL history entries found.".to_string();
    }

    let mut lines = Vec::with_capacity(entries.len() + 2);
    lines.push(format!(
        "{:<24} {:<12} {:<10} {:<20} {}",
        "TIMESTAMP", "SESSION", "DECISION", "TOOL", "TARGET"
    ));
    lines.push("-".repeat(80));

    for e in entries {
        let ts = e.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();
        let sid = if e.session_id.len() > 10 {
            format!("{}...", &e.session_id[..10])
        } else {
            e.session_id.clone()
        };
        lines.push(format!(
            "{:<24} {:<12} {:<10} {:<20} {}",
            ts, sid, e.decision, e.tool_name, e.target_path,
        ));
    }

    lines.push(String::new());
    lines.push(format!("{} entries total.", entries.len()));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a HITL audit entry as JSON.
    fn make_hitl_entry(
        event_type: &str,
        session_id: &str,
        tool_name: &str,
        target_path: &str,
        ts: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "timestamp": ts,
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {
                event_type: {
                    "session_id": session_id,
                    "tool_name": tool_name,
                    "target_path": target_path,
                    "timestamp": ts
                }
            }
        })
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn extract_history_returns_all_hitl_events() {
        let entries = vec![
            make_hitl_entry(
                "HitlApproved",
                "s1",
                "write_file",
                "/tmp/foo.rs",
                "2026-04-20T10:00:00Z",
            ),
            make_hitl_entry(
                "HitlDenied",
                "s1",
                "run_command",
                "",
                "2026-04-20T10:01:00Z",
            ),
            make_hitl_entry(
                "HitlSkipped",
                "s1",
                "write_file",
                "/tmp/bar.rs",
                "2026-04-20T10:02:00Z",
            ),
        ];

        let query = HistoryQuery::default();
        let result = extract_history(&entries, &query);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].decision, Decision::Approved);
        assert_eq!(result[1].decision, Decision::Denied);
        assert_eq!(result[2].decision, Decision::Skipped);
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn extract_history_filters_by_session() {
        let entries = vec![
            make_hitl_entry(
                "HitlApproved",
                "session-abc",
                "write_file",
                "/tmp/a.rs",
                "2026-04-20T10:00:00Z",
            ),
            make_hitl_entry(
                "HitlDenied",
                "session-xyz",
                "run_command",
                "",
                "2026-04-20T10:01:00Z",
            ),
        ];

        let query = HistoryQuery {
            session_id: Some("session-abc".to_string()),
            denied_only: false,
        };
        let result = extract_history(&entries, &query);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].session_id, "session-abc");
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn extract_history_denied_only_filter() {
        let entries = vec![
            make_hitl_entry(
                "HitlApproved",
                "s1",
                "write_file",
                "/tmp/a.rs",
                "2026-04-20T10:00:00Z",
            ),
            make_hitl_entry(
                "HitlDenied",
                "s1",
                "run_command",
                "",
                "2026-04-20T10:01:00Z",
            ),
            make_hitl_entry(
                "HitlSkipped",
                "s1",
                "write_file",
                "/tmp/b.rs",
                "2026-04-20T10:02:00Z",
            ),
        ];

        let query = HistoryQuery {
            session_id: None,
            denied_only: true,
        };
        let result = extract_history(&entries, &query);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].decision, Decision::Denied);
        assert_eq!(result[0].tool_name, "run_command");
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn extract_history_combined_session_and_denied() {
        let entries = vec![
            make_hitl_entry(
                "HitlDenied",
                "s1",
                "write_file",
                "/a",
                "2026-04-20T10:00:00Z",
            ),
            make_hitl_entry(
                "HitlDenied",
                "s2",
                "write_file",
                "/b",
                "2026-04-20T10:01:00Z",
            ),
            make_hitl_entry(
                "HitlApproved",
                "s1",
                "read_file",
                "/c",
                "2026-04-20T10:02:00Z",
            ),
        ];

        let query = HistoryQuery {
            session_id: Some("s1".to_string()),
            denied_only: true,
        };
        let result = extract_history(&entries, &query);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target_path, "/a");
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn extract_history_empty_input() {
        let query = HistoryQuery::default();
        let result = extract_history(&[], &query);
        assert!(result.is_empty());
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn extract_history_skips_non_hitl_events() {
        let non_hitl = serde_json::json!({
            "timestamp": "2026-04-20T10:00:00Z",
            "event": {
                "SessionStarted": {
                    "session_id": "s1"
                }
            }
        });
        let hitl = make_hitl_entry(
            "HitlApproved",
            "s1",
            "write_file",
            "/tmp/a.rs",
            "2026-04-20T10:01:00Z",
        );

        let query = HistoryQuery::default();
        let result = extract_history(&[non_hitl, hitl], &query);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].decision, Decision::Approved);
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn format_history_empty_entries() {
        let output = format_history(&[]);
        assert_eq!(output, "No HITL history entries found.");
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn format_history_produces_table() {
        let entries = vec![HistoryEntry {
            timestamp: "2026-04-20T10:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            session_id: "sess-001".to_string(),
            tool_name: "write_file".to_string(),
            decision: Decision::Approved,
            target_path: "/tmp/foo.rs".to_string(),
        }];

        let output = format_history(&entries);
        assert!(output.contains("TIMESTAMP"));
        assert!(output.contains("DECISION"));
        assert!(output.contains("APPROVED"));
        assert!(output.contains("write_file"));
        assert!(output.contains("/tmp/foo.rs"));
        assert!(output.contains("1 entries total."));
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn format_history_truncates_long_session_id() {
        let entries = vec![HistoryEntry {
            timestamp: "2026-04-20T10:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            session_id: "abcdefghijklmnop".to_string(),
            tool_name: "write_file".to_string(),
            decision: Decision::Denied,
            target_path: "/tmp/x".to_string(),
        }];

        let output = format_history(&entries);
        assert!(output.contains("abcdefghij..."));
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn decision_display() {
        assert_eq!(format!("{}", Decision::Approved), "APPROVED");
        assert_eq!(format!("{}", Decision::Denied), "DENIED");
        assert_eq!(format!("{}", Decision::Skipped), "SKIPPED");
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn parse_entry_missing_timestamp_returns_none() {
        let entry = serde_json::json!({
            "event": {
                "HitlApproved": {
                    "session_id": "s1",
                    "tool_name": "write_file",
                    "target_path": "/tmp/a"
                }
            }
        });
        assert!(parse_history_entry(&entry).is_none());
    }

    // rtmx:req REQ-HITL-006
    #[test]
    fn parse_entry_missing_tool_name_defaults() {
        let entry = serde_json::json!({
            "timestamp": "2026-04-20T10:00:00Z",
            "event": {
                "HitlApproved": {
                    "session_id": "s1"
                }
            }
        });
        let he = parse_history_entry(&entry).unwrap();
        assert_eq!(he.tool_name, "unknown");
        assert_eq!(he.target_path, "");
    }
}
