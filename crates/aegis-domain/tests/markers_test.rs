//! Integration tests for REQ-RTMX-004: Test marker scanning.

use aegis_domain::rtmx::scan_markers;
use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn create_temp_dir() -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("aegis_marker_test_{}_{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// rtmx:req REQ-RTMX-004
#[test]
fn test_rust_marker_scanning() {
    let dir = create_temp_dir();
    let file_path = dir.join("sample.rs");
    fs::write(
        &file_path,
        r#"
// rtmx:req REQ-BUILD-001
#[test]
fn test_binary_build() {
    assert!(true);
}

// @req REQ-TUI-001
#[test]
fn test_tui_layout() {
    assert!(true);
}
"#,
    )
    .unwrap();

    let results = scan_markers(&dir);
    assert_eq!(results.len(), 2, "Should find two markers");

    let build = results
        .iter()
        .find(|r| r.req_id == "REQ-BUILD-001")
        .unwrap();
    assert!(build.file_path.ends_with("sample.rs"));
    assert_eq!(build.function_name.as_deref(), Some("test_binary_build"));

    let tui = results.iter().find(|r| r.req_id == "REQ-TUI-001").unwrap();
    assert_eq!(tui.function_name.as_deref(), Some("test_tui_layout"));

    let _ = fs::remove_dir_all(&dir);
}

// rtmx:req REQ-RTMX-004
#[test]
fn test_marker_formats() {
    let dir = create_temp_dir();
    let file_path = dir.join("formats.rs");
    fs::write(
        &file_path,
        r#"
// rtmx:req REQ-AGENT-001
fn test_rtmx_format() {}

// @req REQ-AGENT-002
fn test_at_req_format() {}

#[req(REQ-AGENT-003)]
fn test_attr_format() {}
"#,
    )
    .unwrap();

    let results = scan_markers(&dir);
    assert_eq!(results.len(), 3, "Should detect all three marker formats");

    let ids: Vec<&str> = results.iter().map(|r| r.req_id.as_str()).collect();
    assert!(ids.contains(&"REQ-AGENT-001"), "rtmx:req format");
    assert!(ids.contains(&"REQ-AGENT-002"), "@req format");
    assert!(ids.contains(&"REQ-AGENT-003"), "#[req()] format");

    let _ = fs::remove_dir_all(&dir);
}

// rtmx:req REQ-RTMX-004
#[test]
fn test_function_name_extraction() {
    let dir = create_temp_dir();
    let file_path = dir.join("funcs.rs");
    fs::write(
        &file_path,
        r#"
mod tests {
    // rtmx:req REQ-TEST-001
    #[test]
    fn my_test_function() {
        assert!(true);
    }

    // rtmx:req REQ-TEST-002
    // This is a comment between marker and function
    fn another_function() {}
}
"#,
    )
    .unwrap();

    let results = scan_markers(&dir);
    assert_eq!(results.len(), 2);

    let test1 = results.iter().find(|r| r.req_id == "REQ-TEST-001").unwrap();
    assert_eq!(test1.function_name.as_deref(), Some("my_test_function"));

    let test2 = results.iter().find(|r| r.req_id == "REQ-TEST-002").unwrap();
    assert_eq!(test2.function_name.as_deref(), Some("another_function"));

    let _ = fs::remove_dir_all(&dir);
}

// rtmx:req REQ-RTMX-004
#[test]
fn test_recursive_scanning() {
    let dir = create_temp_dir();
    let sub = dir.join("subdir");
    fs::create_dir_all(&sub).unwrap();

    fs::write(
        dir.join("top.rs"),
        "// rtmx:req REQ-TOP-001\nfn top_fn() {}\n",
    )
    .unwrap();
    fs::write(
        sub.join("nested.rs"),
        "// rtmx:req REQ-NESTED-001\nfn nested_fn() {}\n",
    )
    .unwrap();

    let results = scan_markers(&dir);
    assert_eq!(results.len(), 2, "Should scan files in subdirectories");

    let _ = fs::remove_dir_all(&dir);
}

// rtmx:req REQ-RTMX-004
#[test]
fn test_line_numbers_are_correct() {
    let dir = create_temp_dir();
    fs::write(
        dir.join("lines.rs"),
        "// line 1\n// line 2\n// rtmx:req REQ-LINE-001\nfn at_line_3() {}\n",
    )
    .unwrap();

    let results = scan_markers(&dir);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].line_number, 3, "Marker is on line 3 (1-indexed)");

    let _ = fs::remove_dir_all(&dir);
}
