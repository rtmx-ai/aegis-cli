//! CUI marking pattern detection and endpoint classification.
//!
//! Provides regex-based detection of Controlled Unclassified Information (CUI)
//! markings and classification of network endpoints as government or commercial.

use regex::{Regex, RegexBuilder};

/// A CUI marking pattern with its name and compiled regex.
#[derive(Debug, Clone)]
pub struct CuiPattern {
    /// Human-readable name of the marking (e.g., "CUI", "FOUO", "NOFORN").
    pub name: String,
    /// Compiled regex pattern (case-insensitive).
    pub regex: Regex,
}

/// A match found by [`scan_for_cui`] indicating a CUI marking in text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuiMatch {
    /// Name of the CUI pattern that matched (e.g., "CUI_BANNER", "FOUO").
    pub pattern_name: String,
    /// Byte offset of the match start in the input text.
    pub start: usize,
    /// Byte offset of the match end (exclusive) in the input text.
    pub end: usize,
    /// The literal text that was matched.
    pub matched_text: String,
}

/// Scan `text` for all CUI markings, returning every match found.
///
/// Applies every pattern from [`cui_patterns`] and collects all
/// non-overlapping matches. Supports multi-line input.
pub fn scan_for_cui(text: &str) -> Vec<CuiMatch> {
    let patterns = cui_patterns();
    let mut matches = Vec::new();
    for pat in &patterns {
        for m in pat.regex.find_iter(text) {
            matches.push(CuiMatch {
                pattern_name: pat.name.clone(),
                start: m.start(),
                end: m.end(),
                matched_text: m.as_str().to_string(),
            });
        }
    }
    // Sort by position for deterministic output.
    matches.sort_by_key(|m| (m.start, m.end));
    matches
}

/// Classification of a network endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointClass {
    /// Government endpoint (.mil, .gov, govcloud).
    Government,
    /// Commercial endpoint (all others).
    Commercial,
}

/// Configuration for endpoint classification.
#[derive(Debug, Clone, Default)]
pub struct EndpointConfig {
    /// Additional domains to classify as Government.
    pub government_allowlist: Vec<String>,
}

/// Standard CUI marking patterns as compiled case-insensitive regexes.
///
/// Covers banner markings (standalone) and portion markings (inline parenthesized).
pub const CUI_PATTERNS: &[(&str, &str)] = &[
    ("CUI_BANNER", r"\bCUI\b"),
    ("CUI_SPECIFIED", r"\bCUI//SP-[A-Z]+\b"),
    ("FOUO", r"\bFOUO\b"),
    ("NOFORN", r"\bNOFORN\b"),
    ("REL_TO", r"\bREL\s+TO\b"),
    ("ORCON", r"\bORCON\b"),
    ("PROPIN", r"\bPROPIN\b"),
    ("PORTION_MARKING", r"\(CUI\)|\(FOUO\)|\(NOFORN\)"),
];

/// Returns compiled CUI marking patterns with case-insensitive matching.
pub fn cui_patterns() -> Vec<CuiPattern> {
    CUI_PATTERNS
        .iter()
        .map(|(name, pattern)| {
            let regex = RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .unwrap_or_else(|e| panic!("invalid CUI pattern '{name}': {e}"));
            CuiPattern {
                name: name.to_string(),
                regex,
            }
        })
        .collect()
}

/// Classify an endpoint URL as Government or Commercial.
///
/// Government endpoints are those with domains ending in `.mil`, `.gov`,
/// or containing `govcloud`. Additional government domains can be specified
/// via `EndpointConfig::government_allowlist`.
pub fn classify_endpoint(url: &str) -> EndpointClass {
    classify_endpoint_with_config(url, &EndpointConfig::default())
}

/// Classify an endpoint URL with a custom configuration.
pub fn classify_endpoint_with_config(url: &str, config: &EndpointConfig) -> EndpointClass {
    let lower = url.to_lowercase();

    // Extract the host portion from the URL.
    let host = extract_host(&lower);

    if host.ends_with(".mil") || host.ends_with(".gov") {
        return EndpointClass::Government;
    }

    if host.contains("govcloud") {
        return EndpointClass::Government;
    }

    for domain in &config.government_allowlist {
        let domain_lower = domain.to_lowercase();
        if host == domain_lower || host.ends_with(&format!(".{domain_lower}")) {
            return EndpointClass::Government;
        }
    }

    EndpointClass::Commercial
}

/// Extract the host from a URL string, stripping scheme, port, and path.
fn extract_host(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let without_path = without_scheme.split('/').next().unwrap_or(without_scheme);
    let without_port = without_path.split(':').next().unwrap_or(without_path);

    without_port.to_string()
}

/// Result of checking whether a message may be transmitted to an endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CuiGateResult {
    /// No CUI markings found, or endpoint is government -- transmission allowed.
    Allowed,
    /// CUI markings detected and endpoint is commercial -- transmission blocked.
    Blocked {
        /// CUI markings found in the message.
        matches: Vec<CuiMatch>,
        /// The endpoint URL that was classified as commercial.
        endpoint: String,
    },
}

/// Check whether `message` may be transmitted to `endpoint_url`.
///
/// Scans the message for CUI markings and classifies the endpoint. If any CUI
/// markings are found and the endpoint is commercial, the transmission is blocked.
pub fn check_cui_gate(message: &str, endpoint_url: &str) -> CuiGateResult {
    check_cui_gate_with_config(message, endpoint_url, &EndpointConfig::default())
}

/// Check whether `message` may be transmitted to `endpoint_url` using a custom
/// endpoint configuration.
pub fn check_cui_gate_with_config(
    message: &str,
    endpoint_url: &str,
    config: &EndpointConfig,
) -> CuiGateResult {
    let matches = scan_for_cui(message);
    if matches.is_empty() {
        return CuiGateResult::Allowed;
    }

    let class = classify_endpoint_with_config(endpoint_url, config);
    match class {
        EndpointClass::Government => CuiGateResult::Allowed,
        EndpointClass::Commercial => CuiGateResult::Blocked {
            matches,
            endpoint: endpoint_url.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-SECURITY-020
    #[test]
    fn test_cui_patterns_match_standard_markings() {
        let patterns = cui_patterns();
        let test_cases = vec![
            ("CUI_BANNER", "This document is CUI"),
            ("CUI_SPECIFIED", "Marked as CUI//SP-PRVCY"),
            ("FOUO", "FOUO document here"),
            ("NOFORN", "NOFORN restricted"),
            ("REL_TO", "REL TO USA, GBR"),
            ("ORCON", "ORCON controlled"),
            ("PROPIN", "PROPIN material"),
            ("PORTION_MARKING", "This is (CUI) text"),
        ];

        for (name, text) in test_cases {
            let pattern = patterns
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("pattern '{name}' not found"));
            assert!(
                pattern.regex.is_match(text),
                "pattern '{name}' should match: {text}"
            );
        }
    }

    // rtmx:req REQ-SECURITY-020
    #[test]
    fn test_cui_patterns_case_insensitive() {
        let patterns = cui_patterns();
        let cui_banner = patterns
            .iter()
            .find(|p| p.name == "CUI_BANNER")
            .expect("CUI_BANNER pattern");

        assert!(cui_banner.regex.is_match("CUI"));
        assert!(cui_banner.regex.is_match("cui"));
        assert!(cui_banner.regex.is_match("Cui"));
        assert!(cui_banner.regex.is_match("cUi"));
    }

    // rtmx:req REQ-SECURITY-020
    #[test]
    fn test_cui_patterns_no_false_positives() {
        let patterns = cui_patterns();
        let cui_banner = patterns
            .iter()
            .find(|p| p.name == "CUI_BANNER")
            .expect("CUI_BANNER pattern");

        // "circuit" contains "cui" but not as a word boundary match.
        assert!(
            !cui_banner.regex.is_match("circuit board"),
            "'circuit' should not match CUI_BANNER"
        );
        assert!(
            !cui_banner.regex.is_match("biscuit"),
            "'biscuit' should not match CUI_BANNER"
        );
    }

    // rtmx:req REQ-SECURITY-020
    #[test]
    fn test_portion_markings_detected() {
        let patterns = cui_patterns();
        let portion = patterns
            .iter()
            .find(|p| p.name == "PORTION_MARKING")
            .expect("PORTION_MARKING pattern");

        assert!(portion.regex.is_match("(CUI) This paragraph is controlled"));
        assert!(portion.regex.is_match("(FOUO) For official use only"));
        assert!(portion.regex.is_match("(NOFORN) No foreign nationals"));
        // Without parens should not match portion marking pattern.
        assert!(!portion.regex.is_match("CUI without parens"));
    }

    // rtmx:req REQ-SECURITY-020
    #[test]
    fn test_cui_specified_with_category() {
        let patterns = cui_patterns();
        let specified = patterns
            .iter()
            .find(|p| p.name == "CUI_SPECIFIED")
            .expect("CUI_SPECIFIED pattern");

        assert!(specified.regex.is_match("CUI//SP-PRVCY"));
        assert!(specified.regex.is_match("CUI//SP-ITAR"));
        assert!(specified.regex.is_match("cui//sp-EXPT"));
        // Missing category should not match.
        assert!(!specified.regex.is_match("CUI//SP-"));
    }

    // rtmx:req REQ-SECURITY-022
    #[test]
    fn test_endpoint_classification() {
        assert_eq!(
            classify_endpoint("https://api.defense.mil/v1"),
            EndpointClass::Government
        );
        assert_eq!(
            classify_endpoint("https://portal.agency.gov/api"),
            EndpointClass::Government
        );
        assert_eq!(
            classify_endpoint("https://api.openai.com/v1"),
            EndpointClass::Commercial
        );
        assert_eq!(
            classify_endpoint("https://example.io/api"),
            EndpointClass::Commercial
        );
    }

    // rtmx:req REQ-SECURITY-022
    #[test]
    fn test_govcloud_endpoint() {
        assert_eq!(
            classify_endpoint("https://us-east1-govcloud.googleapis.com/v1"),
            EndpointClass::Government
        );
        assert_eq!(
            classify_endpoint("https://bedrock.us-govcloud.aws.amazon.com/v1"),
            EndpointClass::Government
        );
    }

    // rtmx:req REQ-SECURITY-022
    #[test]
    fn test_endpoint_allowlist() {
        let config = EndpointConfig {
            government_allowlist: vec!["internal.defense-corp.net".to_string()],
        };

        assert_eq!(
            classify_endpoint_with_config("https://internal.defense-corp.net/api", &config),
            EndpointClass::Government
        );
        assert_eq!(
            classify_endpoint_with_config("https://example.com/api", &config),
            EndpointClass::Commercial
        );
    }

    // rtmx:req REQ-SECURITY-022
    #[test]
    fn test_endpoint_classification_case_insensitive() {
        assert_eq!(
            classify_endpoint("https://portal.agency.GOV/api"),
            EndpointClass::Government
        );
        assert_eq!(
            classify_endpoint("https://api.defense.MIL/v1"),
            EndpointClass::Government
        );
        assert_eq!(
            classify_endpoint("https://US-GOVCLOUD.example.com/v1"),
            EndpointClass::Government
        );
    }

    // rtmx:req REQ-SECURITY-021
    #[test]
    fn test_scanner_finds_cui_in_text() {
        let matches = scan_for_cui("This document is CUI and FOUO restricted.");
        assert!(
            !matches.is_empty(),
            "Scanner must detect CUI markings in text"
        );

        let names: Vec<&str> = matches.iter().map(|m| m.pattern_name.as_str()).collect();
        assert!(names.contains(&"CUI_BANNER"), "Should find CUI_BANNER");
        assert!(names.contains(&"FOUO"), "Should find FOUO");

        // Verify position data is sensible
        for m in &matches {
            assert!(m.start < m.end, "start must precede end");
            assert_eq!(
                &"This document is CUI and FOUO restricted."[m.start..m.end],
                m.matched_text,
                "matched_text must correspond to byte offsets"
            );
        }
    }

    // rtmx:req REQ-SECURITY-021
    #[test]
    fn test_scanner_clean_text_returns_empty() {
        let matches = scan_for_cui("Just a regular sentence with no markings.");
        assert!(
            matches.is_empty(),
            "Clean text without CUI markings should return empty Vec"
        );
    }

    // rtmx:req REQ-SECURITY-021
    #[test]
    fn test_scanner_multiple_markings_in_text() {
        let text = "(CUI) First paragraph.\n(FOUO) Second paragraph.\nNOFORN applies.";
        let matches = scan_for_cui(text);

        let names: Vec<&str> = matches.iter().map(|m| m.pattern_name.as_str()).collect();
        assert!(
            names.contains(&"PORTION_MARKING"),
            "Should detect (CUI) portion marking"
        );
        assert!(names.contains(&"FOUO"), "Should detect FOUO");
        assert!(names.contains(&"NOFORN"), "Should detect NOFORN");

        // Sorted by position
        for w in matches.windows(2) {
            assert!(
                w[0].start <= w[1].start,
                "Matches must be sorted by position"
            );
        }
    }

    // rtmx:req REQ-SECURITY-021
    #[test]
    fn test_scanner_multiline() {
        let text = "Line one\nCUI//SP-ITAR on line two\nLine three with ORCON";
        let matches = scan_for_cui(text);

        let names: Vec<&str> = matches.iter().map(|m| m.pattern_name.as_str()).collect();
        assert!(
            names.contains(&"CUI_SPECIFIED"),
            "Should find CUI_SPECIFIED across lines"
        );
        assert!(
            names.contains(&"ORCON"),
            "Should find ORCON on a later line"
        );
    }

    // rtmx:req REQ-SECURITY-023
    #[test]
    fn test_gate_blocks_cui_to_commercial() {
        let result = check_cui_gate("This document is CUI", "https://api.openai.com/v1");
        match result {
            CuiGateResult::Blocked { matches, endpoint } => {
                assert!(!matches.is_empty(), "Should have CUI matches");
                assert_eq!(endpoint, "https://api.openai.com/v1");
            }
            CuiGateResult::Allowed => {
                panic!("CUI text to commercial endpoint must be blocked");
            }
        }
    }

    // rtmx:req REQ-SECURITY-023
    #[test]
    fn test_gate_allows_cui_to_government() {
        let result = check_cui_gate("This document is CUI", "https://api.defense.mil/v1");
        assert_eq!(
            result,
            CuiGateResult::Allowed,
            "CUI text to government endpoint should be allowed"
        );
    }

    // rtmx:req REQ-SECURITY-023
    #[test]
    fn test_gate_allows_clean_text_to_commercial() {
        let result = check_cui_gate(
            "Just a regular message with no markings",
            "https://api.openai.com/v1",
        );
        assert_eq!(
            result,
            CuiGateResult::Allowed,
            "Clean text to commercial endpoint should be allowed"
        );
    }

    // rtmx:req REQ-SECURITY-023
    #[test]
    fn test_gate_blocked_result_contains_multiple_matches() {
        let result = check_cui_gate(
            "(CUI) First section. FOUO material. NOFORN applies.",
            "https://example.com/api",
        );
        match result {
            CuiGateResult::Blocked { matches, .. } => {
                assert!(
                    matches.len() >= 3,
                    "Should detect at least 3 CUI markings, found {}",
                    matches.len()
                );
                let names: Vec<&str> = matches.iter().map(|m| m.pattern_name.as_str()).collect();
                assert!(names.contains(&"PORTION_MARKING"));
                assert!(names.contains(&"FOUO"));
                assert!(names.contains(&"NOFORN"));
            }
            CuiGateResult::Allowed => {
                panic!("Multiple CUI markings to commercial must be blocked");
            }
        }
    }
}
