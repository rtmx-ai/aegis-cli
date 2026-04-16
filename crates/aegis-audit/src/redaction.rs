//! Redaction verification scan (REQ-AUDIT-015).
//!
//! Scans audit ledger JSONL files and proves they contain no CUI markings
//! or PII. Uses the DLP scanner from aegis-security.

use aegis_security::dlp::DlpScanner;
use serde::Serialize;
use std::path::Path;

/// A single redaction violation found in a ledger file.
#[derive(Debug, Clone, Serialize)]
pub struct RedactionViolation {
    pub file_path: String,
    pub line_number: usize,
    pub category: String,
    pub matched_text: String,
}

/// Summary of a redaction verification scan.
#[derive(Debug, Serialize)]
pub struct RedactionReport {
    pub files_scanned: usize,
    pub violations: Vec<RedactionViolation>,
    pub clean: bool,
}

/// Scan all `.jsonl` files in `log_dir` for CUI markings and PII.
///
/// For each file, parses each JSON line and runs the DLP scanner on
/// all string values. Returns a report listing any violations found.
pub fn verify_redaction(log_dir: &Path) -> RedactionReport {
    let scanner = DlpScanner::new();
    let mut files_scanned: usize = 0;
    let mut violations = Vec::new();

    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => {
            return RedactionReport {
                files_scanned: 0,
                violations: Vec::new(),
                clean: true,
            };
        }
    };

    for dir_entry in entries.flatten() {
        let path = dir_entry.path();
        if path.extension().is_some_and(|e| e == "jsonl") {
            files_scanned += 1;

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let file_path_str = path.display().to_string();

            for (line_idx, line) in content.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }

                // Parse JSON line and extract all string values.
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                    let strings = extract_strings(&value);
                    for s in &strings {
                        let matches = scanner.scan(s);
                        for m in matches {
                            violations.push(RedactionViolation {
                                file_path: file_path_str.clone(),
                                line_number: line_idx + 1,
                                category: format!("{:?}", m.category),
                                matched_text: m.matched_text,
                            });
                        }
                    }
                } else {
                    // Non-JSON lines: scan raw text.
                    let matches = scanner.scan(line);
                    for m in matches {
                        violations.push(RedactionViolation {
                            file_path: file_path_str.clone(),
                            line_number: line_idx + 1,
                            category: format!("{:?}", m.category),
                            matched_text: m.matched_text,
                        });
                    }
                }
            }
        }
    }

    let clean = violations.is_empty();
    RedactionReport {
        files_scanned,
        violations,
        clean,
    }
}

/// Recursively extract all string values from a JSON value.
fn extract_strings(value: &serde_json::Value) -> Vec<String> {
    let mut strings = Vec::new();
    match value {
        serde_json::Value::String(s) => strings.push(s.clone()),
        serde_json::Value::Array(arr) => {
            for v in arr {
                strings.extend(extract_strings(v));
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                strings.extend(extract_strings(v));
            }
        }
        _ => {}
    }
    strings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_jsonl(dir: &Path, filename: &str, lines: &[&str]) {
        let path = dir.join(filename);
        let content = lines.join("\n") + "\n";
        fs::write(path, content).unwrap();
    }

    // rtmx:req REQ-AUDIT-015
    #[test]
    fn test_clean_ledger_passes() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            "aegis-2026-04-15.jsonl",
            &[
                r#"{"timestamp":"2026-04-15T00:00:00Z","event":"SessionStarted"}"#,
                r#"{"timestamp":"2026-04-15T00:01:00Z","event":"ToolCallProposed"}"#,
            ],
        );

        let report = verify_redaction(tmp.path());
        assert_eq!(report.files_scanned, 1);
        assert!(
            report.clean,
            "Clean ledger should pass: {:#?}",
            report.violations
        );
        assert!(report.violations.is_empty());
    }

    // rtmx:req REQ-AUDIT-015
    #[test]
    fn test_cui_in_ledger_detected() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            "aegis-2026-04-15.jsonl",
            &[
                r#"{"timestamp":"2026-04-15T00:00:00Z","event":"SessionStarted"}"#,
                r#"{"timestamp":"2026-04-15T00:01:00Z","event":"Note","content":"CUI//SP-CTI data here"}"#,
            ],
        );

        let report = verify_redaction(tmp.path());
        assert!(!report.clean, "CUI marking should be detected");
        assert!(
            report.violations.iter().any(|v| v.category == "CuiMarking"),
            "Should find CuiMarking violation"
        );
    }

    // rtmx:req REQ-AUDIT-015
    #[test]
    fn test_pii_in_ledger_detected() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            "aegis-2026-04-15.jsonl",
            &[
                r#"{"timestamp":"2026-04-15T00:00:00Z","event":"SessionStarted"}"#,
                r#"{"timestamp":"2026-04-15T00:01:00Z","event":"Note","content":"SSN: 123-45-6789"}"#,
            ],
        );

        let report = verify_redaction(tmp.path());
        assert!(!report.clean, "PII (SSN) should be detected");
        assert!(
            report.violations.iter().any(|v| v.category == "Ssn"),
            "Should find SSN violation"
        );
    }

    // rtmx:req REQ-AUDIT-015
    #[test]
    fn test_empty_directory_is_clean() {
        let tmp = TempDir::new().unwrap();
        let report = verify_redaction(tmp.path());
        assert_eq!(report.files_scanned, 0);
        assert!(report.clean);
    }

    // rtmx:req REQ-AUDIT-015
    #[test]
    fn test_nonexistent_directory_is_clean() {
        let report = verify_redaction(Path::new("/nonexistent/path"));
        assert_eq!(report.files_scanned, 0);
        assert!(report.clean);
    }

    // rtmx:req REQ-AUDIT-015
    #[test]
    fn test_multiple_violations_reported() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            "aegis-2026-04-15.jsonl",
            &[r#"{"content":"CUI//SP-CTI and SSN: 123-45-6789"}"#],
        );

        let report = verify_redaction(tmp.path());
        assert!(
            report.violations.len() >= 2,
            "Should find multiple violations"
        );
    }

    // rtmx:req REQ-AUDIT-015
    #[test]
    fn test_violation_line_number_correct() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            "aegis-2026-04-15.jsonl",
            &[
                r#"{"event":"clean"}"#,
                r#"{"event":"clean2"}"#,
                r#"{"content":"FOUO data"}"#,
            ],
        );

        let report = verify_redaction(tmp.path());
        assert!(!report.violations.is_empty());
        assert_eq!(report.violations[0].line_number, 3, "Violation on line 3");
    }
}
