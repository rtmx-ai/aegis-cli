//! ROI (Return on Investment) estimation from audit ledger data.
//!
//! Scans JSONL ledger files for work-output events (tool calls, requirement
//! links, sessions) and estimates what the equivalent human labor would cost,
//! producing a formatted ROI report.
//!
//! Implements REQ-AUDIT-023a through REQ-AUDIT-023d.

use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// REQ-AUDIT-023a: Work output metrics extraction
// ---------------------------------------------------------------------------

/// Aggregate counts of work performed, extracted from audit ledger events.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkOutputMetrics {
    /// Number of write_file tool calls.
    pub files_written: u64,
    /// Estimated total lines changed across all write_file calls.
    pub lines_changed: u64,
    /// Number of test files created (paths matching `*_test.rs` or `*_tests.rs`).
    pub tests_created: u64,
    /// Number of RequirementLinked events.
    pub requirements_completed: u64,
    /// Tool call counts by tool type (e.g. "WriteFile" -> 5).
    pub tool_calls_by_type: HashMap<String, u64>,
    /// Total number of distinct sessions seen.
    pub sessions: u64,
    /// Wall-clock time span in seconds (last timestamp - first timestamp).
    pub wall_clock_seconds: f64,
}

/// Scan all `.jsonl` files in `logs_dir` and extract work output metrics.
///
/// Looks for:
/// - `ToolCallProposed` events to count tool calls and file writes
/// - `RequirementLinked` events
/// - `SessionStarted` / `SessionEnded` for session counting and timing
pub fn scan_work_outputs(logs_dir: &Path) -> WorkOutputMetrics {
    let entries = match std::fs::read_dir(logs_dir) {
        Ok(e) => e,
        Err(_) => return WorkOutputMetrics::default(),
    };

    let mut metrics = WorkOutputMetrics::default();
    let mut session_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut timestamps: Vec<String> = Vec::new();

    for dir_entry in entries.flatten() {
        let path = dir_entry.path();
        if path.extension().is_some_and(|e| e == "jsonl") {
            scan_work_outputs_file(&path, &mut metrics, &mut session_ids, &mut timestamps);
        }
    }

    metrics.sessions = session_ids.len() as u64;

    // Compute wall-clock time from earliest to latest timestamp.
    if timestamps.len() >= 2 {
        timestamps.sort();
        if let (Some(first), Some(last)) = (timestamps.first(), timestamps.last()) {
            metrics.wall_clock_seconds = timestamp_diff_seconds(first, last);
        }
    }

    metrics
}

/// Parse a single JSONL file and accumulate work output metrics.
fn scan_work_outputs_file(
    path: &Path,
    metrics: &mut WorkOutputMetrics,
    session_ids: &mut std::collections::HashSet<String>,
    timestamps: &mut Vec<String>,
) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        let Some(event) = entry.get("event") else {
            continue;
        };

        // Collect timestamps for wall-clock calculation.
        if let Some(ts) = entry.get("timestamp").and_then(|v| v.as_str()) {
            timestamps.push(ts.to_string());
        }

        // ToolCallProposed: count tool types, file writes, lines, tests.
        if let Some(tcp) = event.get("ToolCallProposed") {
            if let Some(sid) = tcp.get("session_id") {
                collect_session_id(sid, session_ids);
            }
            if let Some(tool_call) = tcp.get("tool_call") {
                process_tool_call(tool_call, metrics);
            }
        }

        // RequirementLinked: count completed requirements.
        if let Some(rl) = event.get("RequirementLinked") {
            metrics.requirements_completed += 1;
            if let Some(sid) = rl.get("session_id") {
                collect_session_id(sid, session_ids);
            }
        }

        // SessionStarted: track sessions.
        if let Some(ss) = event.get("SessionStarted")
            && let Some(sid) = ss.get("session_id")
        {
            collect_session_id(sid, session_ids);
        }

        // SessionEnded: track sessions.
        if let Some(se) = event.get("SessionEnded")
            && let Some(sid) = se.get("session_id")
        {
            collect_session_id(sid, session_ids);
        }
    }
}

/// Extract a session ID string from various JSON representations.
/// SessionId can be serialized as a string or as `{"0": "uuid"}`.
fn collect_session_id(
    value: &serde_json::Value,
    session_ids: &mut std::collections::HashSet<String>,
) {
    if let Some(s) = value.as_str() {
        session_ids.insert(s.to_string());
    } else if let Some(obj) = value.as_object()
        && let Some(inner) = obj.get("0").and_then(|v| v.as_str())
    {
        session_ids.insert(inner.to_string());
    }
}

/// Process a single ToolCall JSON value and update metrics.
fn process_tool_call(tool_call: &serde_json::Value, metrics: &mut WorkOutputMetrics) {
    // ToolCall is a serde tagged enum: {"WriteFile": {"path": ..., "content": ...}}
    if let Some(obj) = tool_call.as_object() {
        for (tool_type, payload) in obj {
            *metrics
                .tool_calls_by_type
                .entry(tool_type.clone())
                .or_insert(0) += 1;

            if tool_type == "WriteFile" {
                metrics.files_written += 1;

                // Count lines in content.
                if let Some(content) = payload.get("content").and_then(|v| v.as_str()) {
                    metrics.lines_changed += content.lines().count() as u64;
                }

                // Check if this is a test file.
                if let Some(path_val) = payload.get("path") {
                    let path_str = path_val.as_str().unwrap_or("");
                    if path_str.ends_with("_test.rs") || path_str.ends_with("_tests.rs") {
                        metrics.tests_created += 1;
                    }
                }
            }
        }
    }
}

/// Compute the difference in seconds between two ISO-8601 timestamp strings.
/// Returns 0.0 if either timestamp cannot be parsed.
fn timestamp_diff_seconds(first: &str, last: &str) -> f64 {
    let parse = |ts: &str| -> Option<f64> {
        // Simple ISO-8601 parsing: try chrono if available, else manual.
        chrono::DateTime::parse_from_rfc3339(ts)
            .ok()
            .map(|dt| dt.timestamp() as f64)
    };
    match (parse(first), parse(last)) {
        (Some(a), Some(b)) => (b - a).max(0.0),
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// REQ-AUDIT-023b: Defense labor rate table and role mapping
// ---------------------------------------------------------------------------

/// Role identifiers for defense labor categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// GS-13 Step 5 software engineer.
    Engineer,
    /// GS-14 site reliability engineer.
    Sre,
    /// Information System Security Engineer.
    Isse,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Engineer => write!(f, "Engineer (GS-13)"),
            Role::Sre => write!(f, "SRE (GS-14)"),
            Role::Isse => write!(f, "ISSE"),
        }
    }
}

/// A single labor rate entry: base hourly and loaded (with overhead) hourly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaborRate {
    pub base_hourly: f64,
    pub loaded_hourly: f64,
}

/// Defense labor rate table mapping roles to hourly rates.
///
/// Default rates are based on OPM GS pay scales with standard overhead loading.
/// Users can override via config.
#[derive(Debug, Clone, PartialEq)]
pub struct LaborRateTable {
    pub rates: HashMap<Role, LaborRate>,
    /// Contractor multiplier applied on top of loaded rates.
    pub contractor_multiplier: f64,
}

impl Default for LaborRateTable {
    fn default() -> Self {
        let mut rates = HashMap::new();
        rates.insert(
            Role::Engineer,
            LaborRate {
                base_hourly: 85.0,
                loaded_hourly: 130.0,
            },
        );
        rates.insert(
            Role::Sre,
            LaborRate {
                base_hourly: 95.0,
                loaded_hourly: 145.0,
            },
        );
        rates.insert(
            Role::Isse,
            LaborRate {
                base_hourly: 110.0,
                loaded_hourly: 168.0,
            },
        );
        Self {
            rates,
            contractor_multiplier: 1.8,
        }
    }
}

impl LaborRateTable {
    /// Create a new rate table from config overrides.
    ///
    /// Accepts a map of role name -> (base, loaded) rate pairs, plus an
    /// optional contractor multiplier. Missing roles fall back to defaults.
    pub fn from_config(
        overrides: &HashMap<String, (f64, f64)>,
        contractor_multiplier: Option<f64>,
    ) -> Self {
        let mut table = Self::default();

        for (role_name, (base, loaded)) in overrides {
            let role = match role_name.to_lowercase().as_str() {
                "engineer" | "gs-13" | "gs13" => Some(Role::Engineer),
                "sre" | "gs-14" | "gs14" => Some(Role::Sre),
                "isse" => Some(Role::Isse),
                _ => None,
            };
            if let Some(r) = role {
                table.rates.insert(
                    r,
                    LaborRate {
                        base_hourly: *base,
                        loaded_hourly: *loaded,
                    },
                );
            }
        }

        if let Some(mult) = contractor_multiplier {
            table.contractor_multiplier = mult;
        }

        table
    }

    /// Get the loaded hourly rate for a role, or 0.0 if not found.
    pub fn loaded_rate(&self, role: Role) -> f64 {
        self.rates
            .get(&role)
            .map(|r| r.loaded_hourly)
            .unwrap_or(0.0)
    }

    /// Get the contractor rate (loaded * multiplier) for a role.
    pub fn contractor_rate(&self, role: Role) -> f64 {
        self.loaded_rate(role) * self.contractor_multiplier
    }
}

// ---------------------------------------------------------------------------
// REQ-AUDIT-023c: Work-to-hours heuristic engine
// ---------------------------------------------------------------------------

/// Estimated human-equivalent hours for a single work category.
#[derive(Debug, Clone, PartialEq)]
pub struct HoursBreakdown {
    pub role: Role,
    pub hours: f64,
    pub cost: f64,
    pub description: String,
    /// Confidence weight 0.0-1.0 for this heuristic.
    pub confidence: f64,
}

/// Full human-equivalent labor estimate derived from work output metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct HumanEquivalentEstimate {
    pub total_hours: f64,
    pub total_cost: f64,
    pub breakdown: Vec<HoursBreakdown>,
    pub by_role: HashMap<Role, (f64, f64)>, // (hours, cost)
    /// Aggregate confidence level 0.0-1.0 (weighted average).
    pub confidence_level: f64,
}

/// Convert work output metrics to a human-equivalent labor estimate.
///
/// Heuristics:
/// - File write: 0.5hr base + 0.01hr per line (Engineer), confidence 0.6
/// - Test file: 0.75hr (Engineer), confidence 0.7
/// - Requirement completion: 4hr (Engineer 2hr + ISSE 2hr), confidence 0.5
/// - run_command: 0.1hr (SRE), confidence 0.8
/// - Security review (implicit per requirement): 1hr (ISSE), confidence 0.4
pub fn estimate_human_equivalent(
    metrics: &WorkOutputMetrics,
    rate_table: &LaborRateTable,
) -> HumanEquivalentEstimate {
    let mut breakdown = Vec::new();
    let mut by_role: HashMap<Role, (f64, f64)> = HashMap::new();

    let eng_rate = rate_table.loaded_rate(Role::Engineer);
    let sre_rate = rate_table.loaded_rate(Role::Sre);
    let isse_rate = rate_table.loaded_rate(Role::Isse);

    // File writes: 0.5hr base + 0.01hr per line (Engineer).
    if metrics.files_written > 0 {
        let hours = (metrics.files_written as f64 * 0.5) + (metrics.lines_changed as f64 * 0.01);
        let cost = hours * eng_rate;
        breakdown.push(HoursBreakdown {
            role: Role::Engineer,
            hours,
            cost,
            description: format!(
                "{} files written, {} lines changed",
                metrics.files_written, metrics.lines_changed
            ),
            confidence: 0.6,
        });
        let entry = by_role.entry(Role::Engineer).or_insert((0.0, 0.0));
        entry.0 += hours;
        entry.1 += cost;
    }

    // Test files: 0.75hr each (Engineer).
    if metrics.tests_created > 0 {
        let hours = metrics.tests_created as f64 * 0.75;
        let cost = hours * eng_rate;
        breakdown.push(HoursBreakdown {
            role: Role::Engineer,
            hours,
            cost,
            description: format!("{} test files created", metrics.tests_created),
            confidence: 0.7,
        });
        let entry = by_role.entry(Role::Engineer).or_insert((0.0, 0.0));
        entry.0 += hours;
        entry.1 += cost;
    }

    // Requirements: 4hr total (2hr Engineer + 2hr ISSE).
    if metrics.requirements_completed > 0 {
        let eng_hours = metrics.requirements_completed as f64 * 2.0;
        let eng_cost = eng_hours * eng_rate;
        breakdown.push(HoursBreakdown {
            role: Role::Engineer,
            hours: eng_hours,
            cost: eng_cost,
            description: format!(
                "{} requirements (engineering)",
                metrics.requirements_completed
            ),
            confidence: 0.5,
        });
        let entry = by_role.entry(Role::Engineer).or_insert((0.0, 0.0));
        entry.0 += eng_hours;
        entry.1 += eng_cost;

        let isse_hours = metrics.requirements_completed as f64 * 2.0;
        let isse_cost = isse_hours * isse_rate;
        breakdown.push(HoursBreakdown {
            role: Role::Isse,
            hours: isse_hours,
            cost: isse_cost,
            description: format!(
                "{} requirements (security review)",
                metrics.requirements_completed
            ),
            confidence: 0.5,
        });
        let entry = by_role.entry(Role::Isse).or_insert((0.0, 0.0));
        entry.0 += isse_hours;
        entry.1 += isse_cost;
    }

    // RunCommand calls: 0.1hr each (SRE).
    let run_commands = metrics
        .tool_calls_by_type
        .get("RunCommand")
        .copied()
        .unwrap_or(0);
    if run_commands > 0 {
        let hours = run_commands as f64 * 0.1;
        let cost = hours * sre_rate;
        breakdown.push(HoursBreakdown {
            role: Role::Sre,
            hours,
            cost,
            description: format!("{} commands executed", run_commands),
            confidence: 0.8,
        });
        let entry = by_role.entry(Role::Sre).or_insert((0.0, 0.0));
        entry.0 += hours;
        entry.1 += cost;
    }

    // Implicit security review per requirement: 1hr ISSE.
    if metrics.requirements_completed > 0 {
        let hours = metrics.requirements_completed as f64 * 1.0;
        let cost = hours * isse_rate;
        breakdown.push(HoursBreakdown {
            role: Role::Isse,
            hours,
            cost,
            description: format!("{} security reviews", metrics.requirements_completed),
            confidence: 0.4,
        });
        let entry = by_role.entry(Role::Isse).or_insert((0.0, 0.0));
        entry.0 += hours;
        entry.1 += cost;
    }

    let total_hours: f64 = breakdown.iter().map(|b| b.hours).sum();
    let total_cost: f64 = breakdown.iter().map(|b| b.cost).sum();

    // Weighted average confidence.
    let confidence_level = if total_hours > 0.0 {
        let weighted_sum: f64 = breakdown.iter().map(|b| b.confidence * b.hours).sum();
        weighted_sum / total_hours
    } else {
        0.0
    };

    HumanEquivalentEstimate {
        total_hours,
        total_cost,
        breakdown,
        by_role,
        confidence_level,
    }
}

// ---------------------------------------------------------------------------
// REQ-AUDIT-023d: ROI report display format
// ---------------------------------------------------------------------------

/// Format a full ROI report showing human-equivalent cost vs. aegis cost.
///
/// `aegis_cost_usd` is the total LLM spend from the cost report.
pub fn format_roi_report(estimate: &HumanEquivalentEstimate, aegis_cost_usd: f64) -> String {
    let mut out = String::from("ROI Estimate\n");
    out.push_str(&format!("{}\n", "-".repeat(60)));

    // Per-role breakdown, sorted for deterministic output.
    let mut roles: Vec<_> = estimate.by_role.iter().collect();
    roles.sort_by_key(|(r, _)| match r {
        Role::Engineer => 0,
        Role::Sre => 1,
        Role::Isse => 2,
    });

    for (role, (hours, cost)) in &roles {
        let rate = if *hours > 0.0 { *cost / *hours } else { 0.0 };
        out.push_str(&format!(
            "  {}: {:.1}h @ ${:.0}/hr = ${:.2}\n",
            role, hours, rate, cost
        ));
    }

    out.push_str(&format!("{}\n", "-".repeat(60)));
    out.push_str(&format!(
        "  Total human-equivalent cost:  ${:.2}\n",
        estimate.total_cost
    ));
    out.push_str(&format!(
        "  aegis LLM cost:               ${:.2}\n",
        aegis_cost_usd
    ));

    let savings_pct = if estimate.total_cost > 0.0 {
        ((estimate.total_cost - aegis_cost_usd) / estimate.total_cost) * 100.0
    } else {
        0.0
    };
    out.push_str(&format!(
        "  Estimated savings:            {:.0}%\n",
        savings_pct
    ));

    out.push_str(&format!(
        "  Confidence:                   {:.0}%\n",
        estimate.confidence_level * 100.0
    ));

    out.push('\n');
    out.push_str(
        "  * Estimates are based on GS pay scale heuristics and may not reflect\n    \
         actual labor costs. Use for directional guidance only.\n",
    );

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a ToolCallProposed ledger line.
    fn make_tool_call_line(
        session_id: &str,
        tool_call: serde_json::Value,
        timestamp: &str,
    ) -> String {
        let entry = serde_json::json!({
            "timestamp": timestamp,
            "os_user": "test",
            "hostname": "host",
            "event": {
                "ToolCallProposed": {
                    "session_id": { "0": session_id },
                    "request_id": { "0": "00000000-0000-0000-0000-000000000001" },
                    "tool_call": tool_call,
                    "timestamp": timestamp
                }
            }
        });
        serde_json::to_string(&entry).unwrap()
    }

    /// Helper: create a RequirementLinked ledger line.
    fn make_req_linked_line(session_id: &str, req_id: &str, timestamp: &str) -> String {
        let entry = serde_json::json!({
            "timestamp": timestamp,
            "os_user": "test",
            "hostname": "host",
            "event": {
                "RequirementLinked": {
                    "session_id": { "0": session_id },
                    "requirement_id": { "0": req_id },
                    "timestamp": timestamp
                }
            }
        });
        serde_json::to_string(&entry).unwrap()
    }

    /// Helper: create a SessionStarted ledger line.
    fn make_session_started_line(session_id: &str, timestamp: &str) -> String {
        let entry = serde_json::json!({
            "timestamp": timestamp,
            "os_user": "test",
            "hostname": "host",
            "event": {
                "SessionStarted": {
                    "session_id": { "0": session_id },
                    "timestamp": timestamp
                }
            }
        });
        serde_json::to_string(&entry).unwrap()
    }

    // rtmx:req REQ-AUDIT-023a
    #[test]
    fn test_work_output_metrics_from_ledger() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("audit.jsonl");

        let lines = [
            make_session_started_line("sess-1", "2026-05-01T10:00:00Z"),
            make_tool_call_line(
                "sess-1",
                serde_json::json!({"WriteFile": {
                    "path": "src/main.rs",
                    "content": "fn main() {\n    println!(\"hello\");\n}\n"
                }}),
                "2026-05-01T10:01:00Z",
            ),
            make_tool_call_line(
                "sess-1",
                serde_json::json!({"WriteFile": {
                    "path": "src/foo_test.rs",
                    "content": "#[test]\nfn it_works() { assert!(true); }\n"
                }}),
                "2026-05-01T10:02:00Z",
            ),
            make_tool_call_line(
                "sess-1",
                serde_json::json!({"RunCommand": {
                    "command": "cargo test",
                    "timeout_secs": 60
                }}),
                "2026-05-01T10:03:00Z",
            ),
            make_tool_call_line(
                "sess-1",
                serde_json::json!({"ReadFile": {"path": "Cargo.toml"}}),
                "2026-05-01T10:04:00Z",
            ),
            make_req_linked_line("sess-1", "REQ-TEST-001", "2026-05-01T10:05:00Z"),
        ];
        let content = lines.join("\n") + "\n";
        std::fs::write(&path, content).unwrap();

        let metrics = scan_work_outputs(dir.path());

        assert_eq!(metrics.files_written, 2);
        assert_eq!(metrics.tests_created, 1);
        assert_eq!(metrics.requirements_completed, 1);
        assert_eq!(metrics.sessions, 1);
        assert_eq!(
            metrics.tool_calls_by_type.get("WriteFile").copied(),
            Some(2)
        );
        assert_eq!(
            metrics.tool_calls_by_type.get("RunCommand").copied(),
            Some(1)
        );
        assert_eq!(metrics.tool_calls_by_type.get("ReadFile").copied(), Some(1));
        // 3 lines in main.rs + 2 lines in test file = 5
        assert_eq!(metrics.lines_changed, 5);
        // Wall clock: 10:00 to 10:05 = 300 seconds
        assert!((metrics.wall_clock_seconds - 300.0).abs() < 1.0);
    }

    // rtmx:req REQ-AUDIT-023a
    #[test]
    fn test_work_output_metrics_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let metrics = scan_work_outputs(dir.path());
        assert_eq!(metrics, WorkOutputMetrics::default());
    }

    // rtmx:req REQ-AUDIT-023b
    #[test]
    fn test_labor_rate_table_default_rates() {
        let table = LaborRateTable::default();

        let eng = table.rates.get(&Role::Engineer).unwrap();
        assert!((eng.base_hourly - 85.0).abs() < f64::EPSILON);
        assert!((eng.loaded_hourly - 130.0).abs() < f64::EPSILON);

        let sre = table.rates.get(&Role::Sre).unwrap();
        assert!((sre.base_hourly - 95.0).abs() < f64::EPSILON);
        assert!((sre.loaded_hourly - 145.0).abs() < f64::EPSILON);

        let isse = table.rates.get(&Role::Isse).unwrap();
        assert!((isse.base_hourly - 110.0).abs() < f64::EPSILON);
        assert!((isse.loaded_hourly - 168.0).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-AUDIT-023b
    #[test]
    fn test_labor_rate_table_contractor_multiplier() {
        let table = LaborRateTable::default();

        assert!((table.contractor_multiplier - 1.8).abs() < f64::EPSILON);

        // Engineer contractor rate: $130 * 1.8 = $234
        let eng_contractor = table.contractor_rate(Role::Engineer);
        assert!((eng_contractor - 234.0).abs() < 1e-10);

        // SRE contractor rate: $145 * 1.8 = $261
        let sre_contractor = table.contractor_rate(Role::Sre);
        assert!((sre_contractor - 261.0).abs() < 1e-10);

        // ISSE contractor rate: $168 * 1.8 = $302.40
        let isse_contractor = table.contractor_rate(Role::Isse);
        assert!((isse_contractor - 302.4).abs() < 1e-10);
    }

    // rtmx:req REQ-AUDIT-023c
    #[test]
    fn test_work_to_hours_for_known_outputs() {
        let metrics = WorkOutputMetrics {
            files_written: 3,
            lines_changed: 100,
            tests_created: 1,
            requirements_completed: 2,
            tool_calls_by_type: {
                let mut m = HashMap::new();
                m.insert("WriteFile".to_string(), 3);
                m.insert("RunCommand".to_string(), 5);
                m.insert("ReadFile".to_string(), 10);
                m
            },
            sessions: 1,
            wall_clock_seconds: 3600.0,
        };
        let table = LaborRateTable::default();
        let estimate = estimate_human_equivalent(&metrics, &table);

        // File writes: 3 * 0.5 + 100 * 0.01 = 1.5 + 1.0 = 2.5h Engineer
        // Tests: 1 * 0.75 = 0.75h Engineer
        // Requirements eng: 2 * 2.0 = 4.0h Engineer
        // Requirements ISSE: 2 * 2.0 = 4.0h ISSE
        // RunCommand: 5 * 0.1 = 0.5h SRE
        // Security review: 2 * 1.0 = 2.0h ISSE
        let expected_eng_hours = 2.5 + 0.75 + 4.0;
        let expected_sre_hours = 0.5;
        let expected_isse_hours = 4.0 + 2.0;
        let expected_total = expected_eng_hours + expected_sre_hours + expected_isse_hours;

        assert!(
            (estimate.total_hours - expected_total).abs() < 0.01,
            "total_hours: {} expected: {}",
            estimate.total_hours,
            expected_total
        );

        let (eng_h, eng_c) = estimate.by_role.get(&Role::Engineer).unwrap();
        assert!((eng_h - expected_eng_hours).abs() < 0.01);
        assert!((eng_c - expected_eng_hours * 130.0).abs() < 0.01);

        let (sre_h, _) = estimate.by_role.get(&Role::Sre).unwrap();
        assert!((sre_h - expected_sre_hours).abs() < 0.01);

        let (isse_h, _) = estimate.by_role.get(&Role::Isse).unwrap();
        assert!((isse_h - expected_isse_hours).abs() < 0.01);

        assert!(estimate.confidence_level > 0.0);
        assert!(estimate.confidence_level <= 1.0);
    }

    // rtmx:req REQ-AUDIT-023c
    #[test]
    fn test_work_to_hours_zero_metrics() {
        let metrics = WorkOutputMetrics::default();
        let table = LaborRateTable::default();
        let estimate = estimate_human_equivalent(&metrics, &table);

        assert!((estimate.total_hours).abs() < f64::EPSILON);
        assert!((estimate.total_cost).abs() < f64::EPSILON);
        assert!(estimate.breakdown.is_empty());
        assert!(estimate.by_role.is_empty());
        assert!((estimate.confidence_level).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-AUDIT-023d
    #[test]
    fn test_roi_report_display_format() {
        let metrics = WorkOutputMetrics {
            files_written: 2,
            lines_changed: 50,
            tests_created: 1,
            requirements_completed: 1,
            tool_calls_by_type: {
                let mut m = HashMap::new();
                m.insert("WriteFile".to_string(), 2);
                m.insert("RunCommand".to_string(), 3);
                m
            },
            sessions: 1,
            wall_clock_seconds: 1800.0,
        };
        let table = LaborRateTable::default();
        let estimate = estimate_human_equivalent(&metrics, &table);
        let report = format_roi_report(&estimate, 5.00);

        assert!(report.contains("ROI Estimate"));
        assert!(report.contains("Engineer (GS-13)"));
        assert!(report.contains("SRE (GS-14)"));
        assert!(report.contains("ISSE"));
        assert!(report.contains("Total human-equivalent cost"));
        assert!(report.contains("aegis LLM cost"));
        assert!(report.contains("Estimated savings"));
        assert!(report.contains("Confidence"));
        assert!(report.contains("Estimates are based on GS pay scale heuristics"));
    }

    // rtmx:req REQ-AUDIT-023d
    #[test]
    fn test_roi_report_shows_savings_percentage() {
        let mut by_role = HashMap::new();
        by_role.insert(Role::Engineer, (10.0, 1300.0));
        let estimate = HumanEquivalentEstimate {
            total_hours: 10.0,
            total_cost: 1300.0,
            breakdown: vec![],
            by_role,
            confidence_level: 0.6,
        };

        let report = format_roi_report(&estimate, 50.0);

        // Savings: (1300 - 50) / 1300 * 100 = 96.15...%
        assert!(report.contains("96%"));
        assert!(report.contains("$1300.00"));
        assert!(report.contains("$50.00"));
    }
}
