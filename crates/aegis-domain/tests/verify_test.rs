//! Integration tests for REQ-RTMX-003: Closed-loop verification.

use aegis_domain::rtmx::{RequirementsDb, VerificationOutcome, verify_requirement};
use std::fs;

const VERIFY_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-V-001,V,X,With test,pass,{TEST_FILE},test_something,Unit Test,TODO,HIGH,1,,
REQ-V-002,V,X,No test,,,,Unit Test,TODO,HIGH,1,,
REQ-V-003,V,X,Bad path,pass,nonexistent/path.rs,test_gone,Unit Test,TODO,HIGH,1,,";

// rtmx:req REQ-RTMX-003
#[test]
fn test_closed_loop_verification() {
    // Create a temporary test file so the path check passes.
    let dir = std::env::temp_dir().join(format!("aegis_verify_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let test_file = dir.join("test_something.rs");
    fs::write(&test_file, "fn test_something() {}").unwrap();

    let csv = VERIFY_CSV.replace("{TEST_FILE}", &test_file.to_string_lossy());
    let db = RequirementsDb::from_csv(&csv).unwrap();

    let result = verify_requirement(&db, "REQ-V-001");
    assert_eq!(result.req_id, "REQ-V-001");
    assert_eq!(result.outcome, VerificationOutcome::Passed);
    assert!(result.test_module.is_some());
    assert!(result.test_function.is_some());

    let _ = fs::remove_dir_all(&dir);
}

// rtmx:req REQ-RTMX-003
#[test]
fn test_no_test_linked() {
    let csv = VERIFY_CSV.replace("{TEST_FILE}", "dummy.rs");
    let db = RequirementsDb::from_csv(&csv).unwrap();

    let result = verify_requirement(&db, "REQ-V-002");
    assert_eq!(result.req_id, "REQ-V-002");
    assert_eq!(result.outcome, VerificationOutcome::NoTestLinked);
    assert!(result.test_module.is_none());
    assert!(result.test_function.is_none());
}

// rtmx:req REQ-RTMX-003
#[test]
fn test_missing_test_file_fails() {
    let csv = VERIFY_CSV.replace("{TEST_FILE}", "dummy.rs");
    let db = RequirementsDb::from_csv(&csv).unwrap();

    let result = verify_requirement(&db, "REQ-V-003");
    assert_eq!(result.req_id, "REQ-V-003");
    match &result.outcome {
        VerificationOutcome::Failed { reason } => {
            assert!(
                reason.contains("not found"),
                "Should mention file not found: {reason}"
            );
        }
        other => panic!("Expected Failed, got {other:?}"),
    }
}

// rtmx:req REQ-RTMX-003
#[test]
fn test_nonexistent_requirement() {
    let csv = VERIFY_CSV.replace("{TEST_FILE}", "dummy.rs");
    let db = RequirementsDb::from_csv(&csv).unwrap();

    let result = verify_requirement(&db, "REQ-FAKE-999");
    assert_eq!(result.req_id, "REQ-FAKE-999");
    match &result.outcome {
        VerificationOutcome::Failed { reason } => {
            assert!(
                reason.contains("not found"),
                "Should mention requirement not found: {reason}"
            );
        }
        other => panic!("Expected Failed, got {other:?}"),
    }
}
