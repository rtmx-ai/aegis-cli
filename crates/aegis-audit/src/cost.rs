//! Cost aggregation from TokensConsumed audit records.
//!
//! Scans JSONL ledger files for `TokensConsumed` events and builds a
//! `CostReport` with breakdowns by session, provider, project, and month.
//! Rate tables are duplicated here to avoid a circular dependency on
//! `aegis-llm`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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

// ---------------------------------------------------------------------------
// Incremental scan cache (REQ-AUDIT-035)
// ---------------------------------------------------------------------------

/// Bookmark tracking how far we have scanned into a single JSONL file.
#[derive(Debug, Clone)]
pub struct FileBookmark {
    /// Byte offset one past the last byte we have already parsed.
    pub last_offset: u64,
    /// Number of `TokensConsumed` records returned from previous scans.
    pub last_line_count: usize,
}

/// In-memory cache of per-file scan bookmarks.  Resets each session.
///
/// After the first full scan of a JSONL file, subsequent calls to
/// [`scan_ledger_files_cached`] only read bytes appended since the
/// previous scan, reducing `/cost` latency from O(all entries) to
/// O(new entries).
#[derive(Debug, Clone, Default)]
pub struct ScanCache {
    bookmarks: HashMap<PathBuf, FileBookmark>,
    /// Accumulated records from all prior scans (the "already-seen" set).
    records: Vec<TokensConsumedRecord>,
}

impl ScanCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a read-only view of cached bookmarks (useful for tests).
    pub fn bookmarks(&self) -> &HashMap<PathBuf, FileBookmark> {
        &self.bookmarks
    }

    /// Return a read-only view of all accumulated records.
    pub fn records(&self) -> &[TokensConsumedRecord] {
        &self.records
    }
}

/// Incrementally scan all `.jsonl` files in `logs_dir`, reading only
/// bytes appended since the last scan recorded in `cache`.
///
/// Returns **all** records seen so far (cached + newly scanned).
pub fn scan_ledger_files_cached(
    logs_dir: &Path,
    cache: &mut ScanCache,
) -> Vec<TokensConsumedRecord> {
    let entries = match std::fs::read_dir(logs_dir) {
        Ok(e) => e,
        Err(_) => return cache.records.clone(),
    };

    for dir_entry in entries.flatten() {
        let path = dir_entry.path();
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let new_records = scan_single_file_from_offset(&path, cache);
        cache.records.extend(new_records);
    }

    cache.records.clone()
}

/// Open `path`, seek to the bookmarked offset (or 0), parse new lines,
/// and update the bookmark in `cache`.  Returns only the **new** records.
fn scan_single_file_from_offset(path: &Path, cache: &mut ScanCache) -> Vec<TokensConsumedRecord> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let canonical = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    };

    let start_offset = cache
        .bookmarks
        .get(&canonical)
        .map(|b| b.last_offset)
        .unwrap_or(0);

    let mut reader = BufReader::new(file);
    if reader.seek(SeekFrom::Start(start_offset)).is_err() {
        return Vec::new();
    }

    let mut records = Vec::new();
    let mut current_offset = start_offset;
    let mut line_buf = String::new();

    loop {
        line_buf.clear();
        let bytes_read = match reader.read_line(&mut line_buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        current_offset += bytes_read as u64;

        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rec) = parse_tokens_consumed_line(trimmed) {
            records.push(rec);
        }
    }

    let prev_count = cache
        .bookmarks
        .get(&canonical)
        .map(|b| b.last_line_count)
        .unwrap_or(0);

    cache.bookmarks.insert(
        canonical,
        FileBookmark {
            last_offset: current_offset,
            last_line_count: prev_count + records.len(),
        },
    );

    records
}

/// Parse a single JSON line into a `TokensConsumedRecord`, if it is one.
fn parse_tokens_consumed_line(line: &str) -> Option<TokensConsumedRecord> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let event = entry.get("event")?;
    let tc = event.get("TokensConsumed")?;

    Some(TokensConsumedRecord {
        session_id: tc.get("session_id")?.as_str()?.to_string(),
        provider_kind: tc.get("provider_kind")?.as_str()?.to_string(),
        model: tc.get("model")?.as_str()?.to_string(),
        project_id: tc
            .get("project_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        region: tc
            .get("region")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        input_tokens: tc.get("input_tokens")?.as_u64()?,
        output_tokens: tc.get("output_tokens")?.as_u64()?,
        timestamp: tc.get("timestamp")?.as_str()?.to_string(),
    })
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
// Token ratio analysis and caching recommendations (REQ-AUDIT-026)
// ---------------------------------------------------------------------------

/// Severity level for a cost recommendation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecommendationSeverity {
    /// Informational observation -- no immediate action required.
    Info,
    /// Deserves attention but is not urgent.
    Warning,
    /// Actionable recommendation that can reduce cost significantly.
    Action,
}

/// A single cost-optimization recommendation derived from token usage data.
#[derive(Debug, Clone)]
pub struct CostRecommendation {
    /// How urgent the recommendation is.
    pub severity: RecommendationSeverity,
    /// Short human-readable title.
    pub title: String,
    /// Detailed explanation with actionable guidance.
    pub detail: String,
    /// Traceability link to the requirement that would address this.
    pub req_ref: String,
}

/// Minimum lifetime input tokens before ratio analysis fires.
/// Below this threshold usage is too small to draw conclusions.
const MIN_INPUT_TOKENS_FOR_RATIO: u64 = 10_000;

/// Ratio above which we recommend prompt caching (Action severity).
const HIGH_RATIO_THRESHOLD: f64 = 5.0;

/// Ratio above which we note elevated input usage (Info severity).
const ELEVATED_RATIO_THRESHOLD: f64 = 3.0;

/// Analyze the input/output token ratio in a [`CostReport`] and return
/// recommendations for reducing cost via prompt caching.
///
/// Rules:
/// - If `total_output` is zero, no ratio can be computed -- return empty.
/// - If `total_input` < [`MIN_INPUT_TOKENS_FOR_RATIO`], skip to avoid noise.
/// - If ratio > 5.0: Action-severity recommendation to enable prompt caching.
/// - If ratio > 3.0: Info-severity observation about elevated input ratio.
pub fn analyze_token_ratio(report: &CostReport) -> Vec<CostRecommendation> {
    let mut recs = Vec::new();

    if report.lifetime.total_output == 0 {
        return recs;
    }
    if report.lifetime.total_input < MIN_INPUT_TOKENS_FOR_RATIO {
        return recs;
    }

    let ratio = report.lifetime.total_input as f64 / report.lifetime.total_output as f64;

    if ratio > HIGH_RATIO_THRESHOLD {
        recs.push(CostRecommendation {
            severity: RecommendationSeverity::Action,
            title: "Enable prompt caching to reduce input token costs".to_string(),
            detail: format!(
                "Lifetime input/output token ratio is {ratio:.1}x (threshold: \
                 {HIGH_RATIO_THRESHOLD:.1}x). This indicates large system prompts \
                 or context blocks are re-sent on every request. Enabling prompt \
                 caching (REQ-LLM-014) can significantly reduce input token \
                 charges. Total input: {}, total output: {}.",
                report.lifetime.total_input, report.lifetime.total_output,
            ),
            req_ref: "REQ-LLM-014".to_string(),
        });
    } else if ratio > ELEVATED_RATIO_THRESHOLD {
        recs.push(CostRecommendation {
            severity: RecommendationSeverity::Info,
            title: "Elevated input/output token ratio".to_string(),
            detail: format!(
                "Lifetime input/output token ratio is {ratio:.1}x (observation \
                 threshold: {ELEVATED_RATIO_THRESHOLD:.1}x). Consider monitoring \
                 this trend; if it continues to rise, prompt caching (REQ-LLM-014) \
                 may become beneficial. Total input: {}, total output: {}.",
                report.lifetime.total_input, report.lifetime.total_output,
            ),
            req_ref: "REQ-LLM-014".to_string(),
        });
    }

    recs
}

/// Format a slice of [`CostRecommendation`] into a human-readable string
/// suitable for terminal display.
///
/// Returns an empty string when there are no recommendations.
pub fn format_recommendations(recs: &[CostRecommendation]) -> String {
    if recs.is_empty() {
        return String::new();
    }

    let mut out = String::from("Cost Recommendations\n");
    for (i, rec) in recs.iter().enumerate() {
        let severity_tag = match rec.severity {
            RecommendationSeverity::Info => "[INFO]",
            RecommendationSeverity::Warning => "[WARNING]",
            RecommendationSeverity::Action => "[ACTION]",
        };
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "  {} {} ({})\n    {}\n",
            severity_tag, rec.title, rec.req_ref, rec.detail,
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Model sizing recommendation (REQ-AUDIT-027)
// ---------------------------------------------------------------------------

/// Average session cost threshold above which we consider the model expensive.
const MODEL_SIZING_AVG_COST_THRESHOLD: f64 = 1.00;

/// Flagship model name fragments (matched case-insensitively).
const FLAGSHIP_FRAGMENTS: &[&str] = &["pro", "opus", "gpt-4o", "sonnet"];

/// For each flagship fragment, a suggested smaller alternative and its
/// approximate blended rate (input+output per million tokens, averaged).
fn alternative_for_flagship(fragment: &str) -> (&'static str, f64) {
    match fragment {
        "pro" => ("gemini-2.5-flash", 0.375), // (0.15+0.60)/2
        "opus" => ("claude-sonnet", 9.0),     // (3+15)/2
        "sonnet" => ("claude-haiku", 2.4),    // (0.80+4.0)/2
        "gpt-4o" => ("gpt-4.1-mini", 1.0),    // (0.40+1.60)/2
        _ => ("a smaller model", 1.0),
    }
}

/// Compute a blended rate for a model (average of input and output per-million
/// rates). Returns `None` if the model is not in the rate table.
fn blended_rate(provider_kind: &str, model: &str) -> Option<f64> {
    let rates = lookup_rates(provider_kind, model)?;
    Some((rates.input_per_million + rates.output_per_million) / 2.0)
}

/// Check per-model average session cost and recommend smaller models for
/// routine tasks when a flagship model exceeds $1.00/session on average.
pub fn analyze_model_sizing(report: &CostReport) -> Vec<CostRecommendation> {
    let mut recs = Vec::new();

    // Group sessions by (provider_kind, model).
    let mut model_groups: HashMap<(String, String), (f64, usize)> = HashMap::new();
    for s in &report.sessions {
        let cost = s.cost_usd.unwrap_or(0.0);
        let entry = model_groups
            .entry((s.provider_kind.clone(), s.model.clone()))
            .or_insert((0.0, 0));
        entry.0 += cost;
        entry.1 += 1;
    }

    for ((provider, model), (total_cost, session_count)) in &model_groups {
        if *session_count == 0 {
            continue;
        }
        let avg_cost = total_cost / *session_count as f64;
        if avg_cost <= MODEL_SIZING_AVG_COST_THRESHOLD {
            continue;
        }

        let model_lower = model.to_lowercase();
        let matched_fragment = FLAGSHIP_FRAGMENTS
            .iter()
            .find(|frag| model_lower.contains(**frag));

        let Some(fragment) = matched_fragment else {
            continue;
        };

        let current_rate = blended_rate(provider, model).unwrap_or(0.0);
        let (alt_name, alt_rate) = alternative_for_flagship(fragment);
        let savings_pct = if current_rate > 0.0 {
            ((current_rate - alt_rate) / current_rate) * 100.0
        } else {
            0.0
        };

        recs.push(CostRecommendation {
            severity: RecommendationSeverity::Warning,
            title: format!("Consider a smaller model for routine tasks ({})", model),
            detail: format!(
                "Average session cost for {} is ${:.2} (threshold: $1.00). \
                 Current blended rate: ${:.2}/M tokens. Alternative {}: \
                 ${:.2}/M tokens (~{:.0}% savings). Route simple prompts \
                 to the smaller model to reduce costs.",
                model, avg_cost, current_rate, alt_name, alt_rate, savings_pct,
            ),
            req_ref: "REQ-AUDIT-027".to_string(),
        });
    }

    recs
}

// ---------------------------------------------------------------------------
// Local model fallback recommendation (REQ-AUDIT-028)
// ---------------------------------------------------------------------------

/// Fraction of sessions from cloud providers above which we suggest local
/// fallback.
const CLOUD_DOMINANCE_THRESHOLD: f64 = 0.80;

/// Minimum total sessions before the local-fallback analysis fires (exclusive).
const MIN_SESSIONS_FOR_LOCAL_FALLBACK: usize = 5;

/// Check whether cloud providers dominate session count and recommend
/// routing simple prompts to a local model for cost savings.
pub fn analyze_local_fallback(report: &CostReport) -> Vec<CostRecommendation> {
    let mut recs = Vec::new();

    let total_sessions = report.lifetime.session_count;
    if total_sessions <= MIN_SESSIONS_FOR_LOCAL_FALLBACK {
        return recs;
    }

    let local_sessions = report
        .by_provider
        .get("local")
        .map(|p| p.session_count)
        .unwrap_or(0);

    let cloud_sessions = total_sessions - local_sessions;
    let cloud_fraction = cloud_sessions as f64 / total_sessions as f64;

    if cloud_fraction <= CLOUD_DOMINANCE_THRESHOLD {
        return recs;
    }

    // Estimate monthly savings: assume 20% of cloud sessions could use local.
    // Monthly projection: if we have N months of data, extrapolate; otherwise
    // use the total as a single-month estimate.
    let month_count = report.by_month.len().max(1) as f64;
    let monthly_cost = report.lifetime.total_cost_usd / month_count;
    let estimated_monthly_savings = monthly_cost * 0.20;

    recs.push(CostRecommendation {
        severity: RecommendationSeverity::Action,
        title: "Route simple prompts to a local model".to_string(),
        detail: format!(
            "Cloud providers account for {:.0}% of sessions ({} of {}), \
             exceeding the {:.0}% threshold. If ~20% of cloud sessions \
             used a local model instead, estimated monthly savings would \
             be ~${:.2}. Configure a local provider (Ollama/vLLM) for \
             routine tasks to reduce cloud spend.",
            cloud_fraction * 100.0,
            cloud_sessions,
            total_sessions,
            CLOUD_DOMINANCE_THRESHOLD * 100.0,
            estimated_monthly_savings,
        ),
        req_ref: "REQ-AUDIT-028".to_string(),
    });

    recs
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

    // rtmx:req REQ-AUDIT-034
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

    // rtmx:req REQ-AUDIT-034
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

    // rtmx:req REQ-AUDIT-034
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

    // rtmx:req REQ-AUDIT-034
    #[test]
    fn test_compute_cost_known_model() {
        // vertex / gemini-2.5-pro: $1.25/M input, $10.00/M output
        let cost = compute_cost("vertex", "gemini-2.5-pro", 1_000_000, 500_000);
        // 1M * $1.25/M + 0.5M * $10.00/M = $1.25 + $5.00 = $6.25
        assert!(cost.is_some());
        assert!((cost.unwrap() - 6.25).abs() < 1e-10);
    }

    // rtmx:req REQ-AUDIT-034
    #[test]
    fn test_compute_cost_unknown_model() {
        let cost = compute_cost("vertex", "nonexistent-xyz-model", 1_000, 1_000);
        assert!(cost.is_none());
    }

    // rtmx:req REQ-AUDIT-034
    #[test]
    fn test_compute_cost_local_is_zero() {
        let cost = compute_cost("local", "llama3", 5_000_000, 5_000_000);
        assert!(cost.is_some());
        assert!((cost.unwrap()).abs() < f64::EPSILON);
    }

    // rtmx:req REQ-AUDIT-034
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

    // rtmx:req REQ-AUDIT-033
    // rtmx:req REQ-AUDIT-034
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

    // rtmx:req REQ-AUDIT-033
    // rtmx:req REQ-AUDIT-034
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

    // rtmx:req REQ-AUDIT-033
    // rtmx:req REQ-AUDIT-034
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

    // rtmx:req REQ-AUDIT-034
    #[test]
    fn test_extract_month_from_timestamp() {
        assert_eq!(extract_month("2026-04-19T00:00:00Z"), "2026-04");
        assert_eq!(extract_month("2025-12-31T23:59:59Z"), "2025-12");
        assert_eq!(extract_month("bad"), "unknown");
    }

    // rtmx:req REQ-AUDIT-034
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

    // -- Incremental scan cache tests (REQ-AUDIT-035) -------------------------

    fn make_ledger_line(session_id: &str, input: u64, output: u64) -> String {
        let entry = serde_json::json!({
            "timestamp": "2026-04-22T00:00:00Z",
            "os_user": "test",
            "hostname": "host",
            "event": {
                "TokensConsumed": {
                    "session_id": session_id,
                    "provider_kind": "vertex",
                    "model": "gemini-2.5-pro",
                    "project_id": null,
                    "region": null,
                    "input_tokens": input,
                    "output_tokens": output,
                    "timestamp": "2026-04-22T00:00:00Z"
                }
            }
        });
        serde_json::to_string(&entry).unwrap()
    }

    // rtmx:req REQ-AUDIT-035
    #[test]
    fn test_scan_cache_first_scan_reads_all() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let content = format!(
            "{}\n{}\n",
            make_ledger_line("s1", 100, 50),
            make_ledger_line("s2", 200, 100)
        );
        std::fs::write(&path, &content).unwrap();

        let mut cache = ScanCache::new();
        let records = scan_ledger_files_cached(dir.path(), &mut cache);

        assert_eq!(records.len(), 2);
        assert_eq!(cache.bookmarks().len(), 1);
        let bm = cache.bookmarks().values().next().unwrap();
        assert_eq!(bm.last_offset, content.len() as u64);
        assert_eq!(bm.last_line_count, 2);
    }

    // rtmx:req REQ-AUDIT-035
    #[test]
    fn test_scan_cache_incremental_reads_only_new() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("ledger.jsonl");

        // Write initial content.
        let line1 = make_ledger_line("s1", 100, 50);
        std::fs::write(&path, format!("{line1}\n")).unwrap();

        let mut cache = ScanCache::new();
        let records = scan_ledger_files_cached(dir.path(), &mut cache);
        assert_eq!(records.len(), 1);

        // Append a second line.
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "{}", make_ledger_line("s2", 200, 100)).unwrap();
        drop(f);

        // Second scan should return all accumulated records (1 old + 1 new).
        let records = scan_ledger_files_cached(dir.path(), &mut cache);
        assert_eq!(records.len(), 2);

        let bm = cache.bookmarks().values().next().unwrap();
        assert_eq!(bm.last_line_count, 2);
    }

    // rtmx:req REQ-AUDIT-035
    #[test]
    fn test_scan_cache_no_new_data_returns_same() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("ledger.jsonl");
        std::fs::write(&path, format!("{}\n", make_ledger_line("s1", 100, 50))).unwrap();

        let mut cache = ScanCache::new();
        let r1 = scan_ledger_files_cached(dir.path(), &mut cache);
        let r2 = scan_ledger_files_cached(dir.path(), &mut cache);

        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        // Bookmark should not have advanced.
        let bm = cache.bookmarks().values().next().unwrap();
        assert_eq!(bm.last_line_count, 1);
    }

    // rtmx:req REQ-AUDIT-035
    #[test]
    fn test_scan_cache_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut cache = ScanCache::new();
        let records = scan_ledger_files_cached(dir.path(), &mut cache);
        assert!(records.is_empty());
        assert!(cache.bookmarks().is_empty());
    }

    // rtmx:req REQ-AUDIT-035
    #[test]
    fn test_scan_cache_multiple_files() {
        let dir = tempfile::TempDir::new().unwrap();

        for i in 0..3 {
            let path = dir.path().join(format!("log-{i}.jsonl"));
            std::fs::write(
                &path,
                format!("{}\n", make_ledger_line(&format!("s{i}"), 100, 50)),
            )
            .unwrap();
        }

        let mut cache = ScanCache::new();
        let records = scan_ledger_files_cached(dir.path(), &mut cache);
        assert_eq!(records.len(), 3);
        assert_eq!(cache.bookmarks().len(), 3);
    }

    // -- Token ratio analysis tests (REQ-AUDIT-026) ---------------------------

    /// Helper: build a CostReport with specified lifetime input/output totals.
    fn report_with_lifetime(input: u64, output: u64) -> CostReport {
        CostReport {
            sessions: Vec::new(),
            by_provider: HashMap::new(),
            by_project: HashMap::new(),
            by_month: HashMap::new(),
            lifetime: PeriodCost {
                total_cost_usd: 0.0,
                total_input: input,
                total_output: output,
                session_count: 1,
            },
        }
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_ratio_above_5_emits_action() {
        // ratio = 60_000 / 10_000 = 6.0 > 5.0
        let report = report_with_lifetime(60_000, 10_000);
        let recs = analyze_token_ratio(&report);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, RecommendationSeverity::Action);
        assert_eq!(recs[0].req_ref, "REQ-LLM-014");
        assert!(recs[0].detail.contains("6.0x"));
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_ratio_between_3_and_5_emits_info() {
        // ratio = 40_000 / 10_000 = 4.0 -- between 3.0 and 5.0
        let report = report_with_lifetime(40_000, 10_000);
        let recs = analyze_token_ratio(&report);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, RecommendationSeverity::Info);
        assert_eq!(recs[0].req_ref, "REQ-LLM-014");
        assert!(recs[0].detail.contains("4.0x"));
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_ratio_below_3_emits_nothing() {
        // ratio = 20_000 / 10_000 = 2.0 -- normal
        let report = report_with_lifetime(20_000, 10_000);
        let recs = analyze_token_ratio(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_ratio_skipped_when_input_below_minimum() {
        // ratio = 6_000 / 1_000 = 6.0, but input < 10_000
        let report = report_with_lifetime(6_000, 1_000);
        let recs = analyze_token_ratio(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_ratio_skipped_when_output_is_zero() {
        let report = report_with_lifetime(100_000, 0);
        let recs = analyze_token_ratio(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_ratio_exactly_at_thresholds() {
        // ratio = 5.0 exactly -- should NOT trigger Action (> 5.0 required)
        let report = report_with_lifetime(50_000, 10_000);
        let recs = analyze_token_ratio(&report);
        // 5.0 is not > 5.0 so falls to the elif: 5.0 > 3.0 => Info
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, RecommendationSeverity::Info);

        // ratio = 3.0 exactly -- should NOT trigger Info (> 3.0 required)
        let report2 = report_with_lifetime(30_000, 10_000);
        let recs2 = analyze_token_ratio(&report2);
        assert!(recs2.is_empty());
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_format_recommendations_empty() {
        let output = format_recommendations(&[]);
        assert!(output.is_empty());
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_format_recommendations_action() {
        let recs = vec![CostRecommendation {
            severity: RecommendationSeverity::Action,
            title: "Enable caching".to_string(),
            detail: "Ratio is high".to_string(),
            req_ref: "REQ-LLM-014".to_string(),
        }];
        let output = format_recommendations(&recs);
        assert!(output.contains("[ACTION]"));
        assert!(output.contains("Enable caching"));
        assert!(output.contains("REQ-LLM-014"));
        assert!(output.contains("Ratio is high"));
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_format_recommendations_multiple() {
        let recs = vec![
            CostRecommendation {
                severity: RecommendationSeverity::Action,
                title: "First".to_string(),
                detail: "Detail 1".to_string(),
                req_ref: "REQ-A".to_string(),
            },
            CostRecommendation {
                severity: RecommendationSeverity::Info,
                title: "Second".to_string(),
                detail: "Detail 2".to_string(),
                req_ref: "REQ-B".to_string(),
            },
        ];
        let output = format_recommendations(&recs);
        assert!(output.contains("[ACTION]"));
        assert!(output.contains("[INFO]"));
        assert!(output.contains("First"));
        assert!(output.contains("Second"));
    }

    // rtmx:req REQ-AUDIT-026
    #[test]
    fn test_analyze_with_real_records() {
        // Integration-style: build records, generate report, analyze ratio.
        let records = vec![
            make_record(
                "s1",
                "vertex",
                "gemini-2.5-pro",
                Some("proj-a"),
                100_000,
                10_000,
                "2026-04-01T00:00:00Z",
            ),
            make_record(
                "s2",
                "vertex",
                "gemini-2.5-pro",
                Some("proj-a"),
                200_000,
                20_000,
                "2026-04-02T00:00:00Z",
            ),
        ];
        let report = build_cost_report(&records);
        // ratio = 300_000 / 30_000 = 10.0
        let recs = analyze_token_ratio(&report);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, RecommendationSeverity::Action);
    }

    // -- Model sizing recommendation tests (REQ-AUDIT-027) --------------------

    /// Helper: build a CostReport with given sessions and computed aggregates.
    fn report_with_sessions(sessions: Vec<SessionCost>) -> CostReport {
        let mut by_provider: HashMap<String, PeriodCost> = HashMap::new();
        let mut lifetime = PeriodCost::default();

        for s in &sessions {
            let cost = s.cost_usd.unwrap_or(0.0);
            accumulate(
                &mut by_provider,
                &s.provider_kind,
                s.input_tokens,
                s.output_tokens,
                cost,
            );
            lifetime.total_cost_usd += cost;
            lifetime.total_input += s.input_tokens;
            lifetime.total_output += s.output_tokens;
            lifetime.session_count += 1;
        }

        CostReport {
            sessions,
            by_provider,
            by_project: HashMap::new(),
            by_month: HashMap::new(),
            lifetime,
        }
    }

    fn session(id: &str, provider: &str, model: &str, input: u64, output: u64) -> SessionCost {
        let cost = compute_cost(provider, model, input, output);
        SessionCost {
            session_id: id.to_string(),
            provider_kind: provider.to_string(),
            model: model.to_string(),
            project_id: None,
            input_tokens: input,
            output_tokens: output,
            cost_usd: cost,
        }
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_no_sessions() {
        let report = report_with_sessions(vec![]);
        let recs = analyze_model_sizing(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_flagship_above_threshold() {
        // vertex/gemini-2.5-pro: $1.25/M input, $10.00/M output
        // 1M input + 500K output = $1.25 + $5.00 = $6.25 per session
        let sessions = vec![
            session("s1", "vertex", "gemini-2.5-pro", 1_000_000, 500_000),
            session("s2", "vertex", "gemini-2.5-pro", 1_000_000, 500_000),
        ];
        let report = report_with_sessions(sessions);
        let recs = analyze_model_sizing(&report);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, RecommendationSeverity::Warning);
        assert_eq!(recs[0].req_ref, "REQ-AUDIT-027");
        assert!(recs[0].detail.contains("gemini-2.5-flash"));
        assert!(recs[0].detail.contains("savings"));
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_flagship_below_threshold() {
        // Small token counts -> cost well below $1.00
        let sessions = vec![
            session("s1", "vertex", "gemini-2.5-pro", 10_000, 5_000),
            session("s2", "vertex", "gemini-2.5-pro", 10_000, 5_000),
        ];
        let report = report_with_sessions(sessions);
        let recs = analyze_model_sizing(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_non_flagship_above_threshold() {
        // haiku is not a flagship model even if cost is high
        // But haiku rate is $0.80/M input + $4.0/M output
        // 10M input + 5M output = $8.00 + $20.00 = $28.00 per session
        let sessions = vec![session(
            "s1",
            "bedrock",
            "claude-haiku-3.5",
            10_000_000,
            5_000_000,
        )];
        let report = report_with_sessions(sessions);
        let recs = analyze_model_sizing(&report);
        // haiku does not match any flagship fragment
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_opus_model() {
        // opus: $15/M input, $75/M output
        // 1M input + 200K output = $15 + $15 = $30 per session
        let sessions = vec![session("s1", "vertex", "claude-opus-4", 1_000_000, 200_000)];
        let report = report_with_sessions(sessions);
        let recs = analyze_model_sizing(&report);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].detail.contains("claude-sonnet"));
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_sonnet_model() {
        // sonnet: $3/M input, $15/M output
        // 1M input + 500K output = $3 + $7.5 = $10.50 per session
        let sessions = vec![session(
            "s1",
            "bedrock",
            "claude-sonnet-4.5",
            1_000_000,
            500_000,
        )];
        let report = report_with_sessions(sessions);
        let recs = analyze_model_sizing(&report);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].detail.contains("claude-haiku"));
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_at_exact_threshold() {
        // avg cost must be > $1.00, not >=
        // We need exactly $1.00 avg. With gemini-2.5-pro ($1.25/M in, $10/M out):
        // We need to find tokens such that cost = $1.00
        // Let's use a known cost model and set cost_usd directly
        let s = SessionCost {
            session_id: "s1".to_string(),
            provider_kind: "vertex".to_string(),
            model: "gemini-2.5-pro".to_string(),
            project_id: None,
            input_tokens: 100_000,
            output_tokens: 50_000,
            cost_usd: Some(1.00), // exactly at threshold
        };
        let report = report_with_sessions(vec![s]);
        let recs = analyze_model_sizing(&report);
        // $1.00 is not > $1.00, so no recommendation
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-027
    #[test]
    fn test_model_sizing_case_insensitive() {
        // Model name with mixed case should still match
        let s = SessionCost {
            session_id: "s1".to_string(),
            provider_kind: "vertex".to_string(),
            model: "Gemini-2.5-PRO".to_string(),
            project_id: None,
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cost_usd: Some(6.25),
        };
        let report = report_with_sessions(vec![s]);
        let recs = analyze_model_sizing(&report);
        assert_eq!(recs.len(), 1);
    }

    // -- Local model fallback tests (REQ-AUDIT-028) ---------------------------

    // rtmx:req REQ-AUDIT-028
    #[test]
    fn test_local_fallback_no_sessions() {
        let report = report_with_sessions(vec![]);
        let recs = analyze_local_fallback(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-028
    #[test]
    fn test_local_fallback_below_session_minimum() {
        // Only 5 sessions -- threshold is > 5
        let sessions: Vec<SessionCost> = (0..5)
            .map(|i| {
                session(
                    &format!("s{i}"),
                    "vertex",
                    "gemini-2.5-pro",
                    100_000,
                    50_000,
                )
            })
            .collect();
        let report = report_with_sessions(sessions);
        let recs = analyze_local_fallback(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-028
    #[test]
    fn test_local_fallback_cloud_dominant() {
        // 6 cloud sessions, 0 local -> 100% cloud
        let sessions: Vec<SessionCost> = (0..6)
            .map(|i| {
                session(
                    &format!("s{i}"),
                    "vertex",
                    "gemini-2.5-pro",
                    100_000,
                    50_000,
                )
            })
            .collect();
        let report = report_with_sessions(sessions);
        let recs = analyze_local_fallback(&report);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, RecommendationSeverity::Action);
        assert_eq!(recs[0].req_ref, "REQ-AUDIT-028");
        assert!(recs[0].detail.contains("100%"));
        assert!(recs[0].detail.contains("local"));
    }

    // rtmx:req REQ-AUDIT-028
    #[test]
    fn test_local_fallback_all_local_sessions() {
        // All sessions are local -> 0% cloud -> no recommendation
        let sessions: Vec<SessionCost> = (0..6)
            .map(|i| session(&format!("s{i}"), "local", "llama3", 100_000, 50_000))
            .collect();
        let report = report_with_sessions(sessions);
        let recs = analyze_local_fallback(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-028
    #[test]
    fn test_local_fallback_at_80_percent_boundary() {
        // 8 cloud + 2 local = 10 sessions, cloud = 80% exactly
        // 80% is NOT > 80%, so no recommendation
        let mut sessions: Vec<SessionCost> = (0..8)
            .map(|i| {
                session(
                    &format!("cloud-{i}"),
                    "vertex",
                    "gemini-2.5-pro",
                    100_000,
                    50_000,
                )
            })
            .collect();
        sessions.extend(
            (0..2).map(|i| session(&format!("local-{i}"), "local", "llama3", 100_000, 50_000)),
        );
        let report = report_with_sessions(sessions);
        let recs = analyze_local_fallback(&report);
        assert!(recs.is_empty());
    }

    // rtmx:req REQ-AUDIT-028
    #[test]
    fn test_local_fallback_above_80_percent() {
        // 9 cloud + 1 local = 10 sessions, cloud = 90% > 80%
        let mut sessions: Vec<SessionCost> = (0..9)
            .map(|i| {
                session(
                    &format!("cloud-{i}"),
                    "vertex",
                    "gemini-2.5-pro",
                    100_000,
                    50_000,
                )
            })
            .collect();
        sessions.push(session("local-0", "local", "llama3", 100_000, 50_000));
        let report = report_with_sessions(sessions);
        let recs = analyze_local_fallback(&report);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].detail.contains("90%"));
    }

    // rtmx:req REQ-AUDIT-028
    #[test]
    fn test_local_fallback_shows_monthly_savings() {
        let sessions: Vec<SessionCost> = (0..6)
            .map(|i| {
                session(
                    &format!("s{i}"),
                    "vertex",
                    "gemini-2.5-pro",
                    1_000_000,
                    500_000,
                )
            })
            .collect();
        let report = report_with_sessions(sessions);
        let recs = analyze_local_fallback(&report);
        assert_eq!(recs.len(), 1);
        // Should mention estimated savings
        assert!(recs[0].detail.contains('$'));
    }

    // rtmx:req REQ-AUDIT-035
    #[test]
    fn test_scan_cache_skips_non_tokens_consumed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("mixed.jsonl");

        let non_token = serde_json::json!({
            "timestamp": "2026-04-22T00:00:00Z",
            "os_user": "test",
            "hostname": "host",
            "event": {
                "SessionStarted": {
                    "session_id": { "0": "00000000-0000-0000-0000-000000000000" },
                    "timestamp": "2026-04-22T00:00:00Z"
                }
            }
        });
        let content = format!(
            "{}\n{}\n",
            serde_json::to_string(&non_token).unwrap(),
            make_ledger_line("s1", 100, 50),
        );
        std::fs::write(&path, &content).unwrap();

        let mut cache = ScanCache::new();
        let records = scan_ledger_files_cached(dir.path(), &mut cache);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "s1");
        // Bookmark offset should cover entire file.
        let bm = cache.bookmarks().values().next().unwrap();
        assert_eq!(bm.last_offset, content.len() as u64);
        // Only 1 TokensConsumed line counted.
        assert_eq!(bm.last_line_count, 1);
    }
}
