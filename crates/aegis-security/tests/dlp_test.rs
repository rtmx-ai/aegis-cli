//! Integration tests for CUI and PII pattern detection (REQ-SECURITY-017).

use aegis_security::dlp::{DlpCategory, DlpScanner};

// -- CUI marking detection (all variants) --

// rtmx:req REQ-SECURITY-017
#[test]
fn cui_sp_cti_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("Document: CUI//SP-CTI"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn cui_sp_expt_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("CUI//SP-EXPT material"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn cui_sp_prvcy_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("CUI//SP-PRVCY"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn controlled_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("CONTROLLED document"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn controlled_noforn_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("CONTROLLED//NOFORN"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn fouo_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("This is FOUO"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn for_official_use_only_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("FOR OFFICIAL USE ONLY"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn unclassified_fouo_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_cui_markings("UNCLASSIFIED//FOUO"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn distribution_statements_a_through_f() {
    let scanner = DlpScanner::new();
    for letter in 'A'..='F' {
        let text = format!("Distribution {letter} -- approved");
        assert!(scanner.has_cui_markings(&text), "Distribution {letter}");
    }
}

// -- PII detection --

// rtmx:req REQ-SECURITY-017
#[test]
fn ssn_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_pii("SSN: 123-45-6789"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn email_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_pii("user@example.com"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn phone_number_detected() {
    let scanner = DlpScanner::new();
    assert!(scanner.has_pii("Call (555) 123-4567"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn credit_card_luhn_valid_detected() {
    let scanner = DlpScanner::new();
    // 4111111111111111 is valid Luhn
    assert!(scanner.has_pii("Card: 4111 1111 1111 1111"));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn credit_card_luhn_invalid_not_detected() {
    let scanner = DlpScanner::new();
    let matches = scanner.scan("Card: 4111 1111 1111 1112");
    assert!(
        !matches
            .iter()
            .any(|m| m.category == DlpCategory::CreditCard),
        "Invalid Luhn should not be flagged as credit card"
    );
}

// rtmx:req REQ-SECURITY-017
#[test]
fn ipv4_address_detected() {
    let scanner = DlpScanner::new();
    let matches = scanner.scan("Server: 10.0.0.1");
    assert!(matches.iter().any(|m| m.category == DlpCategory::IpAddress));
}

// rtmx:req REQ-SECURITY-017
#[test]
fn api_key_detected() {
    let scanner = DlpScanner::new();
    let matches = scanner.scan("api_key=sk_live_abcdefghij1234567890");
    assert!(matches.iter().any(|m| m.category == DlpCategory::ApiKey));
}

// -- False positive resistance --

// rtmx:req REQ-SECURITY-017
#[test]
fn normal_prose_no_false_positives() {
    let scanner = DlpScanner::new();
    let text = "The quick brown fox jumps over the lazy dog. \
                Software development requires careful testing.";
    let matches = scanner.scan(text);
    assert!(
        matches.is_empty(),
        "Normal prose should not trigger: {matches:?}"
    );
}

// rtmx:req REQ-SECURITY-017
#[test]
fn rust_code_no_false_positives() {
    let scanner = DlpScanner::new();
    let text = r#"
fn main() {
    let x: i32 = 42;
    println!("Result: {}", x * 2);
    let v = vec![1, 2, 3];
}
"#;
    let matches = scanner.scan(text);
    assert!(
        matches.is_empty(),
        "Rust code should not trigger: {matches:?}"
    );
}

// rtmx:req REQ-SECURITY-017
#[test]
fn version_number_not_ssn() {
    let scanner = DlpScanner::new();
    // "1.2.3" should not be detected as anything.
    let matches = scanner.scan("Version 1.2.3 released today");
    assert!(
        !matches.iter().any(|m| m.category == DlpCategory::Ssn),
        "Version numbers should not be SSN"
    );
}

// -- Confidence scoring --

// rtmx:req REQ-SECURITY-017
#[test]
fn confidence_values_in_valid_range() {
    let scanner = DlpScanner::new();
    let matches = scanner.scan("CUI//SP-CTI data, SSN: 123-45-6789, user@test.com, FOUO");
    assert!(!matches.is_empty());
    for m in &matches {
        assert!(
            (0.0..=1.0).contains(&m.confidence),
            "confidence {} out of range for {:?}",
            m.confidence,
            m.category,
        );
    }
}

// rtmx:req REQ-SECURITY-017
#[test]
fn cui_marking_has_high_confidence() {
    let scanner = DlpScanner::new();
    let matches = scanner.scan("CUI//SP-CTI");
    let cui_match = matches
        .iter()
        .find(|m| m.category == DlpCategory::CuiMarking)
        .expect("should find CUI match");
    assert!(
        cui_match.confidence >= 0.8,
        "CUI marking confidence should be high; got {}",
        cui_match.confidence,
    );
}
