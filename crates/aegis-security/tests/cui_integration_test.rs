//! Integration tests for CUI gate: endpoint classification and CUI blocking
//! (REQ-SECURITY-025).

use aegis_security::cui::{CuiGateResult, check_cui_gate};

const GOV_ENDPOINT: &str = "https://api.defense.mil/v1/chat";
const COMMERCIAL_ENDPOINT: &str = "https://api.openai.com/v1/chat";
const CUI_TEXT: &str = "CUI // This document is controlled";
const CLEAN_TEXT: &str = "This is a perfectly normal message with no markings.";

// rtmx:req REQ-SECURITY-025
#[test]
fn test_cui_e2e_block_and_allow() {
    // CUI text to commercial endpoint must be blocked.
    let blocked = check_cui_gate(CUI_TEXT, COMMERCIAL_ENDPOINT);
    match &blocked {
        CuiGateResult::Blocked { matches, endpoint } => {
            assert!(
                !matches.is_empty(),
                "Blocked result must contain CUI matches"
            );
            assert_eq!(endpoint, COMMERCIAL_ENDPOINT);
            // Verify match details: at least one match should be the CUI_BANNER pattern.
            let has_cui_banner = matches.iter().any(|m| m.pattern_name == "CUI_BANNER");
            assert!(has_cui_banner, "Should detect CUI_BANNER marking");
            // Verify match text is sensible.
            for m in matches {
                assert!(m.start < m.end, "Match offsets must be valid (start < end)");
                assert!(!m.matched_text.is_empty(), "Matched text must not be empty");
            }
        }
        CuiGateResult::Allowed => {
            panic!("CUI text sent to commercial endpoint must be Blocked, got Allowed");
        }
    }

    // Same CUI text to government endpoint must be allowed.
    let allowed_gov = check_cui_gate(CUI_TEXT, GOV_ENDPOINT);
    assert_eq!(
        allowed_gov,
        CuiGateResult::Allowed,
        "CUI text to government endpoint (.mil) must be Allowed"
    );

    // Clean text to commercial endpoint must be allowed.
    let allowed_clean = check_cui_gate(CLEAN_TEXT, COMMERCIAL_ENDPOINT);
    assert_eq!(
        allowed_clean,
        CuiGateResult::Allowed,
        "Clean text (no CUI markings) to commercial endpoint must be Allowed"
    );
}
