//! Compliance report ZIP bundle for ATO evidence packages (REQ-AUDIT-014).
//!
//! Produces a ZIP archive containing:
//! - `summary.json`: session count, time range, hash chain status, requirement coverage
//! - `events.jsonl`: all audit events for the period
//! - `integrity_check.json`: hash chain verification results per segment
//! - `manifest.json`: file list with SHA-256 hashes of each included file

use crate::hash_chain;
use crate::search::{SearchQuery, SearchResult};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// Configuration for generating a compliance report.
#[derive(Debug)]
pub struct ReportConfig {
    /// Directory containing audit log segments.
    pub log_dir: PathBuf,
    /// Output path for the ZIP bundle.
    pub output_path: PathBuf,
    /// Include only events at or after this timestamp.
    pub since: Option<DateTime<Utc>>,
    /// Include only events at or before this timestamp.
    pub until: Option<DateTime<Utc>>,
}

/// Manifest describing all files in the report ZIP bundle.
#[derive(Debug, Serialize)]
pub struct ReportManifest {
    /// Files included in the ZIP bundle.
    pub files: Vec<ManifestEntry>,
    /// ISO-8601 timestamp when the report was generated.
    pub generated_at: String,
    /// Version of aegis that generated the report.
    pub aegis_version: String,
}

/// A single file entry in the report manifest.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestEntry {
    /// Relative path within the ZIP archive.
    pub path: String,
    /// SHA-256 hex digest of the file contents.
    pub sha256: String,
    /// Size of the file in bytes.
    pub size_bytes: u64,
}

/// Integrity check result for a single log segment.
#[derive(Debug, Serialize)]
struct SegmentIntegrity {
    /// Segment file name.
    segment: String,
    /// Number of entries in the segment.
    entry_count: usize,
    /// Whether the hash chain is valid.
    chain_valid: bool,
    /// Error message if the chain is broken.
    error: Option<String>,
}

/// Overall integrity check results.
#[derive(Debug, Serialize)]
struct IntegrityCheck {
    /// Per-segment results.
    segments: Vec<SegmentIntegrity>,
    /// Whether all segments passed verification.
    all_valid: bool,
}

/// Summary of the compliance report.
#[derive(Debug, Serialize)]
struct ReportSummary {
    /// Total number of events in the report.
    event_count: usize,
    /// Total number of segments scanned.
    segment_count: usize,
    /// Number of unique session IDs found.
    session_count: usize,
    /// Earliest event timestamp (if any).
    time_range_start: Option<String>,
    /// Latest event timestamp (if any).
    time_range_end: Option<String>,
    /// Whether all hash chains passed integrity verification.
    chain_status: String,
    /// Number of unique requirement IDs referenced.
    requirement_count: usize,
}

/// Compute SHA-256 hex digest of a byte slice.
fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Collect all segment paths from the log directory, sorted by name.
fn collect_segment_paths(log_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut segments = Vec::new();

    if !log_dir.exists() {
        return Ok(segments);
    }

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

        if name.ends_with(".jsonl") || name.ends_with(".jsonl.zst") {
            segments.push(path);
        }
    }

    segments.sort();
    Ok(segments)
}

/// Read a segment's lines for hash chain verification.
fn read_segment_lines(path: &Path) -> io::Result<Vec<String>> {
    let content = if crate::rotation::is_compressed(path) {
        crate::rotation::decompress_segment(path)?
    } else {
        std::fs::read_to_string(path)?
    };

    Ok(content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// Run integrity checks on all segments in the log directory.
fn run_integrity_checks(log_dir: &Path) -> io::Result<IntegrityCheck> {
    let segments = collect_segment_paths(log_dir)?;
    let mut results = Vec::new();

    for segment_path in &segments {
        let segment_name = segment_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let lines = match read_segment_lines(segment_path) {
            Ok(l) => l,
            Err(e) => {
                results.push(SegmentIntegrity {
                    segment: segment_name,
                    entry_count: 0,
                    chain_valid: false,
                    error: Some(format!("failed to read segment: {e}")),
                });
                continue;
            }
        };

        let entry_count = lines.len();
        match hash_chain::verify_chain(&lines) {
            Ok(()) => {
                results.push(SegmentIntegrity {
                    segment: segment_name,
                    entry_count,
                    chain_valid: true,
                    error: None,
                });
            }
            Err(e) => {
                results.push(SegmentIntegrity {
                    segment: segment_name,
                    entry_count,
                    chain_valid: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let all_valid = results.iter().all(|r| r.chain_valid);

    Ok(IntegrityCheck {
        segments: results,
        all_valid,
    })
}

/// Extract unique session IDs from search results.
fn count_unique_sessions(result: &SearchResult) -> usize {
    let mut sessions = std::collections::HashSet::new();
    for entry in &result.entries {
        // Check top-level session_id
        if let Some(sid) = entry.get("session_id").and_then(|v| v.as_str()) {
            sessions.insert(sid.to_string());
        }
        // Check inside event variants
        if let Some(event) = entry.get("event").and_then(|v| v.as_object()) {
            for (_key, variant) in event {
                if let Some(sid) = variant.get("session_id").and_then(|v| v.as_str()) {
                    sessions.insert(sid.to_string());
                }
            }
        }
    }
    sessions.len()
}

/// Extract unique requirement IDs from search results.
fn count_unique_requirements(result: &SearchResult) -> usize {
    let mut reqs = std::collections::HashSet::new();
    for entry in &result.entries {
        if let Some(req_id) = entry.get("req_id").and_then(|v| v.as_str()) {
            reqs.insert(req_id.to_string());
        }
    }
    reqs.len()
}

/// Find earliest and latest timestamps in the search results.
fn find_time_range(result: &SearchResult) -> (Option<String>, Option<String>) {
    let mut earliest: Option<String> = None;
    let mut latest: Option<String> = None;

    for entry in &result.entries {
        if let Some(ts) = entry.get("timestamp").and_then(|v| v.as_str()) {
            let ts_str = ts.to_string();
            match &earliest {
                None => earliest = Some(ts_str.clone()),
                Some(e) if ts_str < *e => earliest = Some(ts_str.clone()),
                _ => {}
            }
            match &latest {
                None => latest = Some(ts_str),
                Some(l) if ts_str > *l => latest = Some(ts_str),
                _ => {}
            }
        }
    }

    (earliest, latest)
}

/// Generate a compliance report ZIP bundle.
///
/// Creates a ZIP file at `config.output_path` containing:
/// - `events.jsonl`: all matching audit events
/// - `integrity_check.json`: hash chain verification per segment
/// - `summary.json`: event counts, time range, chain status
/// - `manifest.json`: SHA-256 hashes of all included files
pub fn generate_report(config: &ReportConfig) -> io::Result<ReportManifest> {
    // Search for events using the time filter.
    let query = SearchQuery {
        since: config.since,
        until: config.until,
        ..Default::default()
    };

    let search_result = if config.log_dir.exists() {
        crate::search::search_ledger(&config.log_dir, &query)?
    } else {
        SearchResult {
            entries: Vec::new(),
            total_scanned: 0,
            total_matched: 0,
        }
    };

    // Build events.jsonl content.
    let mut events_content = String::new();
    for entry in &search_result.entries {
        events_content.push_str(
            &serde_json::to_string(entry)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?,
        );
        events_content.push('\n');
    }
    let events_bytes = events_content.as_bytes();

    // Run integrity checks.
    let integrity = run_integrity_checks(&config.log_dir)?;
    let integrity_bytes = serde_json::to_vec_pretty(&integrity)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    // Build summary.
    let (time_start, time_end) = find_time_range(&search_result);
    let segment_count = integrity.segments.len();
    let chain_status = if integrity.all_valid {
        "PASS".to_string()
    } else {
        "FAIL".to_string()
    };

    let summary = ReportSummary {
        event_count: search_result.entries.len(),
        segment_count,
        session_count: count_unique_sessions(&search_result),
        time_range_start: time_start,
        time_range_end: time_end,
        chain_status,
        requirement_count: count_unique_requirements(&search_result),
    };
    let summary_bytes = serde_json::to_vec_pretty(&summary)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    // Compute SHA-256 hashes of the three content files.
    let mut manifest_entries = vec![
        ManifestEntry {
            path: "events.jsonl".to_string(),
            sha256: sha256_hex(events_bytes),
            size_bytes: events_bytes.len() as u64,
        },
        ManifestEntry {
            path: "integrity_check.json".to_string(),
            sha256: sha256_hex(&integrity_bytes),
            size_bytes: integrity_bytes.len() as u64,
        },
        ManifestEntry {
            path: "summary.json".to_string(),
            sha256: sha256_hex(&summary_bytes),
            size_bytes: summary_bytes.len() as u64,
        },
    ];

    // Create the ZIP file.
    let file = std::fs::File::create(&config.output_path)?;
    let mut zip = ZipWriter::new(file);
    let options =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // Write events.jsonl
    zip.start_file("events.jsonl", options)
        .map_err(|e| io::Error::other(e.to_string()))?;
    zip.write_all(events_bytes)?;

    // Write integrity_check.json
    zip.start_file("integrity_check.json", options)
        .map_err(|e| io::Error::other(e.to_string()))?;
    zip.write_all(&integrity_bytes)?;

    // Write summary.json
    zip.start_file("summary.json", options)
        .map_err(|e| io::Error::other(e.to_string()))?;
    zip.write_all(&summary_bytes)?;

    // Build the manifest (including itself).
    let generated_at = Utc::now().to_rfc3339();
    let aegis_version =
        std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string());

    // We need to compute the manifest's own hash after serialization,
    // so we first create a preliminary manifest, serialize it, then
    // add the manifest entry with its hash.
    let manifest = ReportManifest {
        files: manifest_entries.clone(),
        generated_at: generated_at.clone(),
        aegis_version: aegis_version.clone(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    // Add manifest entry for manifest.json itself.
    manifest_entries.push(ManifestEntry {
        path: "manifest.json".to_string(),
        sha256: sha256_hex(&manifest_bytes),
        size_bytes: manifest_bytes.len() as u64,
    });

    // Write manifest.json
    zip.start_file("manifest.json", options)
        .map_err(|e| io::Error::other(e.to_string()))?;
    zip.write_all(&manifest_bytes)?;

    zip.finish().map_err(|e| io::Error::other(e.to_string()))?;

    Ok(ReportManifest {
        files: manifest_entries,
        generated_at,
        aegis_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_chain::{GENESIS_PREV_HASH, compute_entry_hash};
    use serde_json::json;
    use tempfile::TempDir;

    /// Build a valid hash-chained JSONL entry.
    fn build_chained_entry(
        prev_hash: &str,
        event_type: &str,
        session_id: &str,
        ts: &str,
        req_id: Option<&str>,
    ) -> (String, String) {
        let mut body = json!({
            "timestamp": ts,
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {
                event_type: {
                    "session_id": session_id,
                    "timestamp": ts
                }
            }
        });

        if let Some(rid) = req_id {
            body.as_object_mut()
                .unwrap()
                .insert("req_id".to_string(), json!(rid));
        }

        let mut obj = body.as_object().unwrap().clone();
        obj.insert("prev_hash".to_string(), json!(prev_hash));

        let body_json = serde_json::to_string(&body).unwrap();
        let temp_line = format!(
            "{{\"prev_hash\":\"{prev_hash}\",{}}}",
            &body_json[1..body_json.len() - 1]
        );
        let hash = compute_entry_hash(prev_hash, &temp_line);

        obj.insert("entry_hash".to_string(), json!(hash));
        let line = serde_json::to_string(&obj).unwrap();
        (line, hash)
    }

    /// Write a segment file with given lines.
    fn write_segment(dir: &Path, name: &str, lines: &[&str]) {
        let path = dir.join(name);
        let content = lines.join("\n") + "\n";
        std::fs::write(path, content).unwrap();
    }

    /// Write sample chained JSONL entries to a segment file.
    fn write_chained_segment(dir: &Path, name: &str) -> Vec<String> {
        let (e1, h1) = build_chained_entry(
            GENESIS_PREV_HASH,
            "SessionStarted",
            "session-001",
            "2026-04-10T00:00:00Z",
            Some("REQ-AUDIT-001"),
        );
        let (e2, _h2) = build_chained_entry(
            &h1,
            "ToolCallExecuted",
            "session-001",
            "2026-04-10T00:01:00Z",
            None,
        );
        write_segment(dir, name, &[&e1, &e2]);
        vec![e1, e2]
    }

    /// Read and parse a file from inside a ZIP archive.
    fn read_zip_entry(zip_path: &Path, entry_name: &str) -> Vec<u8> {
        let file = std::fs::File::open(zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut entry = archive.by_name(entry_name).unwrap();
        let mut buf = Vec::new();
        io::Read::read_to_end(&mut entry, &mut buf).unwrap();
        buf
    }

    /// List all file names in a ZIP archive.
    fn zip_file_names(zip_path: &Path) -> Vec<String> {
        let file = std::fs::File::open(zip_path).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        (0..archive.len())
            .map(|i| {
                let file = std::fs::File::open(zip_path).unwrap();
                let archive = zip::ZipArchive::new(file).unwrap();
                archive.name_for_index(i).unwrap_or("").to_string()
            })
            .collect()
    }

    // rtmx:req REQ-AUDIT-014
    #[test]
    fn test_generate_report_creates_zip_with_all_files() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        write_chained_segment(&log_dir, "aegis-2026-04-10.jsonl");

        let output = tmp.path().join("report.zip");
        let config = ReportConfig {
            log_dir,
            output_path: output.clone(),
            since: None,
            until: None,
        };

        let manifest = generate_report(&config).unwrap();
        assert!(output.exists(), "ZIP file should be created");

        let names = zip_file_names(&output);
        assert!(
            names.contains(&"events.jsonl".to_string()),
            "ZIP should contain events.jsonl"
        );
        assert!(
            names.contains(&"integrity_check.json".to_string()),
            "ZIP should contain integrity_check.json"
        );
        assert!(
            names.contains(&"summary.json".to_string()),
            "ZIP should contain summary.json"
        );
        assert!(
            names.contains(&"manifest.json".to_string()),
            "ZIP should contain manifest.json"
        );
        assert_eq!(names.len(), 4, "ZIP should contain exactly 4 files");
        assert_eq!(manifest.files.len(), 4, "manifest should list 4 files");
    }

    // rtmx:req REQ-AUDIT-014
    #[test]
    fn test_generate_report_summary_has_correct_counts() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        write_chained_segment(&log_dir, "aegis-2026-04-10.jsonl");

        let output = tmp.path().join("report.zip");
        let config = ReportConfig {
            log_dir,
            output_path: output.clone(),
            since: None,
            until: None,
        };

        generate_report(&config).unwrap();

        let summary_bytes = read_zip_entry(&output, "summary.json");
        let summary: serde_json::Value = serde_json::from_slice(&summary_bytes).unwrap();

        assert_eq!(summary["event_count"], 2, "should have 2 events");
        assert_eq!(summary["segment_count"], 1, "should have 1 segment");
        assert_eq!(summary["session_count"], 1, "should have 1 unique session");
        assert_eq!(
            summary["requirement_count"], 1,
            "should have 1 unique req_id"
        );
        assert!(
            summary["time_range_start"].as_str().is_some(),
            "should have time_range_start"
        );
        assert!(
            summary["time_range_end"].as_str().is_some(),
            "should have time_range_end"
        );
    }

    // rtmx:req REQ-AUDIT-014
    #[test]
    fn test_generate_report_integrity_check_passes() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        write_chained_segment(&log_dir, "aegis-2026-04-10.jsonl");

        let output = tmp.path().join("report.zip");
        let config = ReportConfig {
            log_dir,
            output_path: output.clone(),
            since: None,
            until: None,
        };

        generate_report(&config).unwrap();

        let integrity_bytes = read_zip_entry(&output, "integrity_check.json");
        let integrity: serde_json::Value = serde_json::from_slice(&integrity_bytes).unwrap();

        assert_eq!(
            integrity["all_valid"], true,
            "integrity check should pass for valid chain"
        );
        let segments = integrity["segments"].as_array().unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0]["chain_valid"], true);
        assert_eq!(segments[0]["entry_count"], 2);
    }

    // rtmx:req REQ-AUDIT-014
    #[test]
    fn test_generate_report_manifest_has_sha256() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        write_chained_segment(&log_dir, "aegis-2026-04-10.jsonl");

        let output = tmp.path().join("report.zip");
        let config = ReportConfig {
            log_dir,
            output_path: output.clone(),
            since: None,
            until: None,
        };

        let manifest = generate_report(&config).unwrap();

        for entry in &manifest.files {
            assert_eq!(
                entry.sha256.len(),
                64,
                "SHA-256 hex digest should be 64 chars for {}",
                entry.path
            );
            assert!(
                entry.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "SHA-256 should be valid hex for {}",
                entry.path
            );
            assert!(
                entry.size_bytes > 0 || entry.path == "events.jsonl",
                "file size should be > 0 for {}",
                entry.path
            );
        }

        // Verify that the SHA-256 of events.jsonl matches the actual
        // content in the ZIP.
        let events_bytes = read_zip_entry(&output, "events.jsonl");
        let events_entry = manifest
            .files
            .iter()
            .find(|e| e.path == "events.jsonl")
            .unwrap();
        assert_eq!(
            events_entry.sha256,
            sha256_hex(&events_bytes),
            "manifest SHA-256 should match actual file content"
        );
    }

    // rtmx:req REQ-AUDIT-014
    #[test]
    fn test_generate_report_empty_ledger() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        // No segment files at all.

        let output = tmp.path().join("report.zip");
        let config = ReportConfig {
            log_dir,
            output_path: output.clone(),
            since: None,
            until: None,
        };

        let manifest = generate_report(&config).unwrap();
        assert!(
            output.exists(),
            "ZIP should be created even for empty ledger"
        );

        let summary_bytes = read_zip_entry(&output, "summary.json");
        let summary: serde_json::Value = serde_json::from_slice(&summary_bytes).unwrap();
        assert_eq!(summary["event_count"], 0);
        assert_eq!(summary["segment_count"], 0);
        assert_eq!(summary["session_count"], 0);

        // Manifest should still have 4 files.
        assert_eq!(manifest.files.len(), 4);
    }

    // rtmx:req REQ-AUDIT-014
    #[test]
    fn test_generate_report_with_time_filter() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        // Create entries spanning multiple timestamps (no hash chain
        // needed for time filter test -- just plain JSONL).
        let e1 = json!({
            "timestamp": "2026-04-10T00:00:00Z",
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {"SessionStarted": {"session_id": "s1"}}
        });
        let e2 = json!({
            "timestamp": "2026-04-10T12:00:00Z",
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {"ToolCallExecuted": {"session_id": "s2"}}
        });
        let e3 = json!({
            "timestamp": "2026-04-11T00:00:00Z",
            "os_user": "testuser",
            "hostname": "testhost",
            "event": {"SessionEnded": {"session_id": "s3"}}
        });

        write_segment(
            &log_dir,
            "aegis-2026-04-10.jsonl",
            &[
                &serde_json::to_string(&e1).unwrap(),
                &serde_json::to_string(&e2).unwrap(),
                &serde_json::to_string(&e3).unwrap(),
            ],
        );

        let output = tmp.path().join("report.zip");
        let config = ReportConfig {
            log_dir,
            output_path: output.clone(),
            since: Some("2026-04-10T06:00:00Z".parse::<DateTime<Utc>>().unwrap()),
            until: Some("2026-04-10T18:00:00Z".parse::<DateTime<Utc>>().unwrap()),
        };

        generate_report(&config).unwrap();

        let summary_bytes = read_zip_entry(&output, "summary.json");
        let summary: serde_json::Value = serde_json::from_slice(&summary_bytes).unwrap();
        assert_eq!(
            summary["event_count"], 1,
            "only the event within the time range should be included"
        );
    }
}
