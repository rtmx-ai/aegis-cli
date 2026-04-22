//! Cost aggregation from TokensConsumed audit records.
//!
//! Scans JSONL ledger files for `TokensConsumed` events and builds a
//! `CostReport` with breakdowns by session, provider, project, and month.
//! Rate tables are duplicated here to avoid a circular dependency on
//! `aegis-llm`.

use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Scanner: extract TokensConsumed records from JSONL ledger files
// ---------------------------------------------------------------------------

/// A single token-usage record extracted from a ledger entry.
#[derive(Debug, Clone)]
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

/// Scan all `.jsonl` files in `logs_dir` and return every
/// `TokensConsumed` record found.
pub fn scan_ledger_files(logs_dir: &Path) -> Vec<TokensConsumedRecord> {
    let entries = match std::fs::read_dir(logs_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut records = Vec::new();
    for dir_entry in entries.flatten() {
        let path = dir_entry.path();
        if path.extension().is_some_and(|e| e == "jsonl") {
            records.extend(scan_single_file(&path));
        }
    }
    records
}

/// Scan a single JSONL file and return `TokensConsumed` records.
pub fn scan_single_file(path: &Path) -> Vec<TokensConsumedRecord> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut records = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Each ledger line is: { "timestamp": ..., "os_user": ...,
        //   "hostname": ..., "event": <DomainEvent>, "req_id"?: ... }
        // DomainEvent is a serde tagged enum, so TokensConsumed appears as:
        //   "event": { "TokensConsumed": { ... } }
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(event) = entry.get("event") else {
            continue;
        };
        let Some(tc) = event.get("TokensConsumed") else {
            continue;
        };

        let Some(session_id) = tc.get("session_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(provider_kind) = tc.get("provider_kind").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(model) = tc.get("model").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(input_tokens) = tc.get("input_tokens").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(output_tokens) = tc.get("output_tokens").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(timestamp) = tc.get("timestamp").and_then(|v| v.as_str()) else {
            continue;
        };

        records.push(TokensConsumedRecord {
            session_id: session_id.to_string(),
            provider_kind: provider_kind.to_string(),
            model: model.to_string(),
            project_id: tc
                .get("project_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            region: tc
                .get("region")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            input_tokens,
            output_tokens,
            timestamp: timestamp.to_string(),
        });
    }
    records
}

// ---------------------------------------------------------------------------
// Rate table (duplicated from aegis-llm to avoid circular dependency)
// ---------------------------------------------------------------------------

struct RateEntry {
    input_per_million: f64,
    output_per_million: f64,
}

/// Compute dollar cost for a given provider/model/token-count.
/// Returns `None` if the model is not in the rate table.
/// Returns `Some(0.0)` for local models.
pub fn compute_cost(
    provider_kind: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> Option<f64> {
    let rates = lookup_rates(provider_kind, model)?;
    let input_cost = (input_tokens as f64 / 1_000_000.0) * rates.input_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * rates.output_per_million;
    Some(input_cost + output_cost)
}

fn lookup_rates(provider_kind: &str, model: &str) -> Option<RateEntry> {
    let pk = provider_kind.to_lowercase();
    match pk.as_str() {
        "local" => Some(RateEntry {
            input_per_million: 0.0,
            output_per_million: 0.0,
        }),

        "vertex" if model.contains("gemini") && model.contains("pro") => Some(RateEntry {
            input_per_million: 1.25,
            output_per_million: 10.0,
        }),
        "vertex" if model.contains("gemini") && model.contains("flash") => Some(RateEntry {
            input_per_million: 0.15,
            output_per_million: 0.60,
        }),
        "vertex" if model.contains("claude") && model.contains("opus") => Some(RateEntry {
            input_per_million: 15.0,
            output_per_million: 75.0,
        }),
        "vertex" if model.contains("claude") && model.contains("sonnet") => Some(RateEntry {
            input_per_million: 3.0,
            output_per_million: 15.0,
        }),

        "bedrock" if model.contains("sonnet") => Some(RateEntry {
            input_per_million: 3.0,
            output_per_million: 15.0,
        }),
        "bedrock" if model.contains("haiku") => Some(RateEntry {
            input_per_million: 0.80,
            output_per_million: 4.0,
        }),

        "azure" if model.contains("gpt-4.1") && model.contains("mini") => Some(RateEntry {
            input_per_million: 0.40,
            output_per_million: 1.60,
        }),
        "azure" if model.contains("gpt-4.1") => Some(RateEntry {
            input_per_million: 2.0,
            output_per_million: 8.0,
        }),
        "azure" if model.contains("gpt-5") => Some(RateEntry {
            input_per_million: 2.0,
            output_per_million: 8.0,
        }),
        "azure" if model.contains("o3-mini") => Some(RateEntry {
            input_per_million: 1.10,
            output_per_million: 4.40,
        }),

        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Cost report structures
// ---------------------------------------------------------------------------

/// Cost for a single session.
#[derive(Debug, Clone)]
pub struct SessionCost {
    pub session_id: String,
    pub provider_kind: String,
    pub model: String,
    pub project_id: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
}

/// Aggregated cost for a time period or grouping.
#[derive(Debug, Clone, Default)]
pub struct PeriodCost {
    pub total_cost_usd: f64,
    pub total_input: u64,
    pub total_output: u64,
    pub session_count: usize,
}

/// Full cost report with breakdowns by multiple dimensions.
#[derive(Debug, Clone)]
pub struct CostReport {
    pub sessions: Vec<SessionCost>,
    pub by_provider: HashMap<String, PeriodCost>,
    pub by_project: HashMap<String, PeriodCost>,
    pub by_month: HashMap<String, PeriodCost>,
    pub lifetime: PeriodCost,
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

/// Build a cost report from a slice of `TokensConsumedRecord`.
///
/// Records are grouped by `session_id` to produce per-session costs, then
/// aggregated by provider, project, month, and lifetime.
pub fn build_cost_report(records: &[TokensConsumedRecord]) -> CostReport {
    // Step 1: group records by session_id.
    // Within a session all records should share provider/model/project, but
    // we take them from the first record and sum tokens.
    let mut session_map: HashMap<String, SessionAccum> = HashMap::new();
    for r in records {
        let entry = session_map
            .entry(r.session_id.clone())
            .or_insert_with(|| SessionAccum {
                provider_kind: r.provider_kind.clone(),
                model: r.model.clone(),
                project_id: r.project_id.clone(),
                input_tokens: 0,
                output_tokens: 0,
                timestamp: r.timestamp.clone(),
            });
        entry.input_tokens += r.input_tokens;
        entry.output_tokens += r.output_tokens;
        // Keep earliest timestamp for month bucketing.
        if r.timestamp < entry.timestamp {
            entry.timestamp.clone_from(&r.timestamp);
        }
    }

    // Step 2: convert to SessionCost and compute dollar amounts.
    let mut sessions: Vec<SessionCost> = Vec::with_capacity(session_map.len());
    let mut by_provider: HashMap<String, PeriodCost> = HashMap::new();
    let mut by_project: HashMap<String, PeriodCost> = HashMap::new();
    let mut by_month: HashMap<String, PeriodCost> = HashMap::new();
    let mut lifetime = PeriodCost::default();

    for (session_id, acc) in &session_map {
        let cost_usd = compute_cost(
            &acc.provider_kind,
            &acc.model,
            acc.input_tokens,
            acc.output_tokens,
        );

        let sc = SessionCost {
            session_id: session_id.clone(),
            provider_kind: acc.provider_kind.clone(),
            model: acc.model.clone(),
            project_id: acc.project_id.clone(),
            input_tokens: acc.input_tokens,
            output_tokens: acc.output_tokens,
            cost_usd,
        };
        sessions.push(sc);

        let usd = cost_usd.unwrap_or(0.0);

        // by_provider
        accumulate(
            &mut by_provider,
            &acc.provider_kind,
            acc.input_tokens,
            acc.output_tokens,
            usd,
        );

        // by_project
        let project_key = acc.project_id.as_deref().unwrap_or("(none)").to_string();
        accumulate(
            &mut by_project,
            &project_key,
            acc.input_tokens,
            acc.output_tokens,
            usd,
        );

        // by_month -- extract "YYYY-MM" from timestamp
        let month_key = extract_month(&acc.timestamp);
        accumulate(
            &mut by_month,
            &month_key,
            acc.input_tokens,
            acc.output_tokens,
            usd,
        );

        // lifetime
        lifetime.total_cost_usd += usd;
        lifetime.total_input += acc.input_tokens;
        lifetime.total_output += acc.output_tokens;
        lifetime.session_count += 1;
    }

    CostReport {
        sessions,
        by_provider,
        by_project,
        by_month,
        lifetime,
    }
}

struct SessionAccum {
    provider_kind: String,
    model: String,
    project_id: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    timestamp: String,
}

fn accumulate(
    map: &mut HashMap<String, PeriodCost>,
    key: &str,
    input: u64,
    output: u64,
    cost: f64,
) {
    let entry = map.entry(key.to_string()).or_default();
    entry.total_cost_usd += cost;
    entry.total_input += input;
    entry.total_output += output;
    entry.session_count += 1;
}

/// Extract "YYYY-MM" from an ISO-8601 timestamp string.
/// Falls back to "unknown" for malformed timestamps.
fn extract_month(ts: &str) -> String {
    if ts.len() >= 7 && ts.as_bytes()[4] == b'-' {
        ts[..7].to_string()
    } else {
        "unknown".to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(
        session_id: &str,
        provider_kind: &str,
        model: &str,
        project_id: Option<&str>,
        input_tokens: u64,
        output_tokens: u64,
        timestamp: &str,
    ) -> TokensConsumedRecord {
        TokensConsumedRecord {
            session_id: session_id.to_string(),
            provider_kind: provider_kind.to_string(),
            model: model.to_string(),
            project_id: project_id.map(|s| s.to_string()),
            region: None,
            input_tokens,
            output_tokens,
            timestamp: timestamp.to_string(),
        }
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_cost_report_groups_by_provider_and_project() {
        let records = vec![
            make_record(
                "s1",
                "vertex",
                "gemini-2.5-pro",
                Some("proj-a"),
                1_000_000,
                500_000,
                "2026-04-01T00:00:00Z",
            ),
            make_record(
                "s2",
                "bedrock",
                "claude-sonnet-4.5",
                Some("proj-b"),
                2_000_000,
                1_000_000,
                "2026-04-01T00:00:00Z",
            ),
        ];

        let report = build_cost_report(&records);

        // Two providers
        assert_eq!(report.by_provider.len(), 2);
        assert!(report.by_provider.contains_key("vertex"));
        assert!(report.by_provider.contains_key("bedrock"));

        // Two projects
        assert_eq!(report.by_project.len(), 2);
        assert!(report.by_project.contains_key("proj-a"));
        assert!(report.by_project.contains_key("proj-b"));

        // Verify vertex tokens
        let vertex = &report.by_provider["vertex"];
        assert_eq!(vertex.total_input, 1_000_000);
        assert_eq!(vertex.total_output, 500_000);
        assert_eq!(vertex.session_count, 1);

        // Verify bedrock tokens
        let bedrock = &report.by_provider["bedrock"];
        assert_eq!(bedrock.total_input, 2_000_000);
        assert_eq!(bedrock.total_output, 1_000_000);
        assert_eq!(bedrock.session_count, 1);
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_cost_report_groups_by_month() {
        let records = vec![
            make_record(
                "s1",
                "vertex",
                "gemini-2.5-pro",
                None,
                100_000,
                50_000,
                "2026-03-15T10:00:00Z",
            ),
            make_record(
                "s2",
                "vertex",
                "gemini-2.5-pro",
                None,
                200_000,
                100_000,
                "2026-04-01T10:00:00Z",
            ),
        ];

        let report = build_cost_report(&records);

        assert_eq!(report.by_month.len(), 2);
        assert!(report.by_month.contains_key("2026-03"));
        assert!(report.by_month.contains_key("2026-04"));

        let mar = &report.by_month["2026-03"];
        assert_eq!(mar.total_input, 100_000);
        assert_eq!(mar.session_count, 1);

        let apr = &report.by_month["2026-04"];
        assert_eq!(apr.total_input, 200_000);
        assert_eq!(apr.session_count, 1);
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_cost_report_lifetime_sums_all() {
        let records = vec![
            make_record(
                "s1",
                "vertex",
                "gemini-2.5-pro",
                Some("proj-a"),
                1_000_000,
                500_000,
                "2026-04-01T00:00:00Z",
            ),
            make_record(
                "s2",
                "bedrock",
                "claude-sonnet-4.5",
                Some("proj-b"),
                2_000_000,
                1_000_000,
                "2026-04-02T00:00:00Z",
            ),
        ];

        let report = build_cost_report(&records);

        // Lifetime should sum all sessions
        assert_eq!(report.lifetime.total_input, 3_000_000);
        assert_eq!(report.lifetime.total_output, 1_500_000);
        assert_eq!(report.lifetime.session_count, 2);

        // Lifetime cost should match sum of session costs
        let session_sum: f64 = report.sessions.iter().filter_map(|s| s.cost_usd).sum();
        assert!(
            (report.lifetime.total_cost_usd - session_sum).abs() < 1e-10,
            "lifetime cost {} should equal session sum {}",
            report.lifetime.total_cost_usd,
            session_sum
        );
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_compute_cost_known_model() {
        // vertex / gemini-2.5-pro: $1.25/M input, $10.00/M output
        let cost = compute_cost("vertex", "gemini-2.5-pro", 1_000_000, 500_000);
        // 1M * $1.25/M + 0.5M * $10.00/M = $1.25 + $5.00 = $6.25
        assert!(cost.is_some());
        assert!((cost.unwrap() - 6.25).abs() < 1e-10);
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_compute_cost_unknown_model() {
        let cost = compute_cost("vertex", "nonexistent-xyz-model", 1_000, 1_000);
        assert!(cost.is_none());
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_compute_cost_local_is_zero() {
        let cost = compute_cost("local", "llama3", 5_000_000, 5_000_000);
        assert!(cost.is_some());
        assert!((cost.unwrap()).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_session_aggregation_combines_multiple_records() {
        let records = vec![
            make_record(
                "sess-1",
                "vertex",
                "gemini-2.5-pro",
                Some("proj-x"),
                500_000,
                200_000,
                "2026-04-10T10:00:00Z",
            ),
            make_record(
                "sess-1",
                "vertex",
                "gemini-2.5-pro",
                Some("proj-x"),
                300_000,
                100_000,
                "2026-04-10T10:05:00Z",
            ),
        ];

        let report = build_cost_report(&records);

        assert_eq!(report.sessions.len(), 1);
        let session = &report.sessions[0];
        assert_eq!(session.session_id, "sess-1");
        assert_eq!(session.input_tokens, 800_000);
        assert_eq!(session.output_tokens, 300_000);

        // Cost should be based on combined tokens
        let expected = compute_cost("vertex", "gemini-2.5-pro", 800_000, 300_000);
        assert_eq!(session.cost_usd, expected);
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_scan_single_file_extracts_tokens_consumed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        // Build a ledger entry the way JsonlLedger writes it:
        // { "timestamp": ..., "os_user": ..., "hostname": ..., "event": { "TokensConsumed": { ... } } }
        let entry = serde_json::json!({
            "timestamp": "2026-04-19T00:00:00Z",
            "os_user": "test",
            "hostname": "host",
            "event": {
                "TokensConsumed": {
                    "session_id": "s-abc",
                    "provider_kind": "vertex",
                    "model": "gemini-2.5-pro",
                    "project_id": "my-proj",
                    "region": "us-central1",
                    "input_tokens": 1234,
                    "output_tokens": 567,
                    "timestamp": "2026-04-19T00:00:00Z"
                }
            }
        });
        let line = serde_json::to_string(&entry).unwrap();
        std::fs::write(&path, format!("{line}\n")).unwrap();

        let records = scan_single_file(&path);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "s-abc");
        assert_eq!(records[0].provider_kind, "vertex");
        assert_eq!(records[0].model, "gemini-2.5-pro");
        assert_eq!(records[0].project_id.as_deref(), Some("my-proj"));
        assert_eq!(records[0].input_tokens, 1234);
        assert_eq!(records[0].output_tokens, 567);
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_scan_single_file_skips_non_tokens_consumed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("mixed.jsonl");

        let session_line = serde_json::json!({
            "timestamp": "2026-04-19T00:00:00Z",
            "os_user": "test",
            "hostname": "host",
            "event": {
                "SessionStarted": {
                    "session_id": { "0": "00000000-0000-0000-0000-000000000000" },
                    "timestamp": "2026-04-19T00:00:00Z"
                }
            }
        });
        let token_line = serde_json::json!({
            "timestamp": "2026-04-19T00:00:01Z",
            "os_user": "test",
            "hostname": "host",
            "event": {
                "TokensConsumed": {
                    "session_id": "s-1",
                    "provider_kind": "local",
                    "model": "llama3",
                    "project_id": null,
                    "region": null,
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "timestamp": "2026-04-19T00:00:01Z"
                }
            }
        });

        let content = format!(
            "{}\n{}\n",
            serde_json::to_string(&session_line).unwrap(),
            serde_json::to_string(&token_line).unwrap()
        );
        std::fs::write(&path, content).unwrap();

        let records = scan_single_file(&path);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "s-1");
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_scan_ledger_files_across_multiple_files() {
        let dir = tempfile::TempDir::new().unwrap();

        for (i, session_id) in ["s-a", "s-b"].iter().enumerate() {
            let path = dir.path().join(format!("aegis-2026-04-{:02}.jsonl", i + 1));
            let entry = serde_json::json!({
                "timestamp": format!("2026-04-{:02}T00:00:00Z", i + 1),
                "os_user": "test",
                "hostname": "host",
                "event": {
                    "TokensConsumed": {
                        "session_id": session_id,
                        "provider_kind": "vertex",
                        "model": "gemini-2.5-flash",
                        "project_id": null,
                        "region": null,
                        "input_tokens": 500,
                        "output_tokens": 250,
                        "timestamp": format!("2026-04-{:02}T00:00:00Z", i + 1)
                    }
                }
            });
            let line = serde_json::to_string(&entry).unwrap();
            std::fs::write(&path, format!("{line}\n")).unwrap();
        }

        let records = scan_ledger_files(dir.path());
        assert_eq!(records.len(), 2);
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_extract_month_from_timestamp() {
        assert_eq!(extract_month("2026-04-19T00:00:00Z"), "2026-04");
        assert_eq!(extract_month("2025-12-31T23:59:59Z"), "2025-12");
        assert_eq!(extract_month("bad"), "unknown");
    }

    // rtmx:req REQ-AUDIT-021b
    #[test]
    fn test_empty_records_produce_empty_report() {
        let report = build_cost_report(&[]);
        assert!(report.sessions.is_empty());
        assert!(report.by_provider.is_empty());
        assert!(report.by_project.is_empty());
        assert!(report.by_month.is_empty());
        assert_eq!(report.lifetime.session_count, 0);
        assert!((report.lifetime.total_cost_usd).abs() < f64::EPSILON);
    }
}
