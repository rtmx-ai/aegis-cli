//! Integration tests for redaction verification scan (REQ-AUDIT-015).

use aegis_audit::redaction::verify_redaction;
use std::fs;
use std::path::Path;
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
            r#"{"timestamp":"2026-04-15T00:00:00Z","os_user":"test","hostname":"host","event":"SessionStarted"}"#,
            r#"{"timestamp":"2026-04-15T00:01:00Z","os_user":"test","hostname":"host","event":"ToolCallProposed","tool":"ReadFile","path":"src/main.rs"}"#,
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
            r#"{"timestamp":"2026-04-15T00:01:00Z","event":"Note","content":"This is CUI//SP-CTI data"}"#,
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
        "Should find SSN violation: {:#?}",
        report.violations
    );
}

// rtmx:req REQ-AUDIT-015
#[test]
fn test_fouo_in_ledger_detected() {
    let tmp = TempDir::new().unwrap();
    write_jsonl(
        tmp.path(),
        "aegis-2026-04-15.jsonl",
        &[r#"{"content":"FOUO sensitive document"}"#],
    );

    let report = verify_redaction(tmp.path());
    assert!(!report.clean, "FOUO should be detected");
}

// rtmx:req REQ-AUDIT-015
#[test]
fn test_email_in_ledger_detected() {
    let tmp = TempDir::new().unwrap();
    write_jsonl(
        tmp.path(),
        "aegis-2026-04-15.jsonl",
        &[r#"{"content":"Contact user@example.mil for details"}"#],
    );

    let report = verify_redaction(tmp.path());
    assert!(!report.clean, "Email should be detected");
}

// rtmx:req REQ-AUDIT-015
#[test]
fn test_multiple_files_scanned() {
    let tmp = TempDir::new().unwrap();
    write_jsonl(
        tmp.path(),
        "aegis-2026-04-15.jsonl",
        &[r#"{"event":"clean"}"#],
    );
    write_jsonl(
        tmp.path(),
        "aegis-2026-04-14.jsonl",
        &[r#"{"event":"clean"}"#],
    );

    let report = verify_redaction(tmp.path());
    assert_eq!(report.files_scanned, 2);
    assert!(report.clean);
}

// rtmx:req REQ-AUDIT-015
#[test]
fn test_non_jsonl_files_ignored() {
    let tmp = TempDir::new().unwrap();
    // Write a .txt file with CUI -- should be ignored.
    fs::write(tmp.path().join("notes.txt"), "CUI//SP-CTI").unwrap();
    write_jsonl(
        tmp.path(),
        "aegis-2026-04-15.jsonl",
        &[r#"{"event":"clean"}"#],
    );

    let report = verify_redaction(tmp.path());
    assert_eq!(report.files_scanned, 1);
    assert!(report.clean);
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
fn test_violation_includes_file_path() {
    let tmp = TempDir::new().unwrap();
    write_jsonl(
        tmp.path(),
        "aegis-2026-04-15.jsonl",
        &[r#"{"content":"CUI//SP-CTI"}"#],
    );

    let report = verify_redaction(tmp.path());
    assert!(!report.violations.is_empty());
    assert!(
        report.violations[0]
            .file_path
            .contains("aegis-2026-04-15.jsonl"),
        "Violation should include file path"
    );
}

// rtmx:req REQ-AUDIT-015
#[test]
fn test_nested_json_strings_scanned() {
    let tmp = TempDir::new().unwrap();
    write_jsonl(
        tmp.path(),
        "aegis-2026-04-15.jsonl",
        &[r#"{"event":"ToolResult","output":{"text":"SSN: 123-45-6789"}}"#],
    );

    let report = verify_redaction(tmp.path());
    assert!(!report.clean, "Nested JSON strings should be scanned");
}
