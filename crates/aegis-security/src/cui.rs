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
}
