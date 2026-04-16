//! CUI and PII pattern detection (REQ-SECURITY-017).
//!
//! Regex-based scanner for CUI markings (CUI//SP-CTI, FOUO, etc.)
//! and PII patterns (SSN, email, phone, credit card, API keys).

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Category of detected sensitive content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DlpCategory {
    /// CUI markings: CUI//SP-CTI, CONTROLLED, FOUO, distribution statements.
    CuiMarking,
    /// Social Security Numbers (NNN-NN-NNNN).
    Ssn,
    /// Email addresses.
    Email,
    /// US phone numbers.
    PhoneNumber,
    /// Credit card numbers (Luhn-validated).
    CreditCard,
    /// IPv4 addresses.
    IpAddress,
    /// Common API key patterns.
    ApiKey,
}

/// A single match found during DLP scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpMatch {
    pub category: DlpCategory,
    pub matched_text: String,
    pub line_number: usize,
    pub confidence: f32,
}

/// Compiled DLP pattern for detection.
struct DlpPattern {
    category: DlpCategory,
    regex: Regex,
    confidence: f32,
}

/// Scanner for CUI markings and PII patterns.
pub struct DlpScanner {
    patterns: Vec<DlpPattern>,
}

impl Default for DlpScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl DlpScanner {
    /// Create a scanner with the default pattern library.
    pub fn new() -> Self {
        Self {
            patterns: Self::default_patterns(),
        }
    }

    /// Scan content for CUI markings and PII patterns.
    pub fn scan(&self, content: &str) -> Vec<DlpMatch> {
        let mut matches = Vec::new();

        for (line_idx, line) in content.lines().enumerate() {
            for pattern in &self.patterns {
                for m in pattern.regex.find_iter(line) {
                    let matched_text = m.as_str().to_string();

                    // Credit card numbers require Luhn validation.
                    if pattern.category == DlpCategory::CreditCard {
                        let digits: String = matched_text
                            .chars()
                            .filter(|c| c.is_ascii_digit())
                            .collect();
                        if !luhn_check(&digits) {
                            continue;
                        }
                    }

                    matches.push(DlpMatch {
                        category: pattern.category,
                        matched_text,
                        line_number: line_idx + 1,
                        confidence: pattern.confidence,
                    });
                }
            }
        }

        matches
    }

    /// Check whether content contains any CUI markings.
    pub fn has_cui_markings(&self, content: &str) -> bool {
        let matches = self.scan(content);
        matches
            .iter()
            .any(|m| m.category == DlpCategory::CuiMarking)
    }

    /// Check whether content contains any PII patterns.
    pub fn has_pii(&self, content: &str) -> bool {
        let matches = self.scan(content);
        matches.iter().any(|m| {
            matches!(
                m.category,
                DlpCategory::Ssn
                    | DlpCategory::Email
                    | DlpCategory::PhoneNumber
                    | DlpCategory::CreditCard
            )
        })
    }

    /// Build the default pattern library.
    fn default_patterns() -> Vec<DlpPattern> {
        vec![
            // -- CUI Markings --
            DlpPattern {
                category: DlpCategory::CuiMarking,
                regex: Regex::new(
                    r"(?i)CUI//(SP-CTI|SP-EXPT|SP-PRVCY|SP-[A-Z]{2,10})",
                )
                .expect("valid regex"),
                confidence: 0.95,
            },
            DlpPattern {
                category: DlpCategory::CuiMarking,
                regex: Regex::new(r"(?i)\bCONTROLLED(//NOFORN)?\b").expect("valid regex"),
                confidence: 0.9,
            },
            DlpPattern {
                category: DlpCategory::CuiMarking,
                regex: Regex::new(r"(?i)\bFOUO\b").expect("valid regex"),
                confidence: 0.9,
            },
            DlpPattern {
                category: DlpCategory::CuiMarking,
                regex: Regex::new(r"(?i)\bFOR OFFICIAL USE ONLY\b").expect("valid regex"),
                confidence: 0.95,
            },
            DlpPattern {
                category: DlpCategory::CuiMarking,
                regex: Regex::new(r"(?i)\bUNCLASSIFIED//FOUO\b").expect("valid regex"),
                confidence: 0.95,
            },
            DlpPattern {
                category: DlpCategory::CuiMarking,
                regex: Regex::new(
                    r"(?i)\bDistribution\s+[A-F]\b",
                )
                .expect("valid regex"),
                confidence: 0.85,
            },
            // -- SSN --
            DlpPattern {
                category: DlpCategory::Ssn,
                regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("valid regex"),
                confidence: 0.9,
            },
            // -- Email --
            DlpPattern {
                category: DlpCategory::Email,
                regex: Regex::new(
                    r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
                )
                .expect("valid regex"),
                confidence: 0.85,
            },
            // -- US Phone Numbers --
            DlpPattern {
                category: DlpCategory::PhoneNumber,
                regex: Regex::new(
                    r"\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b",
                )
                .expect("valid regex"),
                confidence: 0.75,
            },
            // -- Credit Card Numbers (13-19 digits, optionally separated) --
            DlpPattern {
                category: DlpCategory::CreditCard,
                regex: Regex::new(
                    r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{1,7}\b",
                )
                .expect("valid regex"),
                confidence: 0.8,
            },
            // -- IPv4 Addresses --
            DlpPattern {
                category: DlpCategory::IpAddress,
                regex: Regex::new(
                    r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b",
                )
                .expect("valid regex"),
                confidence: 0.7,
            },
            // -- API Key patterns --
            DlpPattern {
                category: DlpCategory::ApiKey,
                regex: Regex::new(
                    r#"(?i)(?:api[_-]?key|secret[_-]?key|access[_-]?token)\s*[:=]\s*['"]?[A-Za-z0-9_\-]{20,}['"]?"#,
                )
                .expect("valid regex"),
                confidence: 0.8,
            },
        ]
    }
}

/// Luhn algorithm validation for credit card numbers.
fn luhn_check(digits: &str) -> bool {
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum: u32 = 0;
    let mut double = false;

    for ch in digits.chars().rev() {
        let Some(d) = ch.to_digit(10) else {
            return false;
        };
        let val = if double {
            let doubled = d * 2;
            if doubled > 9 { doubled - 9 } else { doubled }
        } else {
            d
        };
        sum += val;
        double = !double;
    }

    sum.is_multiple_of(10)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- REQ-SECURITY-017: CUI marking detection --

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_cui_sp_cti() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("Document marked CUI//SP-CTI");
        assert!(!matches.is_empty());
        assert!(
            matches
                .iter()
                .any(|m| m.category == DlpCategory::CuiMarking)
        );
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_cui_sp_expt() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("CUI//SP-EXPT content here");
        assert!(
            matches
                .iter()
                .any(|m| m.category == DlpCategory::CuiMarking)
        );
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_cui_sp_prvcy() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("This is CUI//SP-PRVCY data");
        assert!(
            matches
                .iter()
                .any(|m| m.category == DlpCategory::CuiMarking)
        );
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_controlled_marking() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("CONTROLLED document");
        assert!(scanner.has_cui_markings("CONTROLLED document"));
        assert!(!matches.is_empty());
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_controlled_noforn() {
        let scanner = DlpScanner::new();
        assert!(scanner.has_cui_markings("CONTROLLED//NOFORN"));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_fouo() {
        let scanner = DlpScanner::new();
        assert!(scanner.has_cui_markings("This document is FOUO"));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_for_official_use_only() {
        let scanner = DlpScanner::new();
        assert!(scanner.has_cui_markings("FOR OFFICIAL USE ONLY"));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_unclassified_fouo() {
        let scanner = DlpScanner::new();
        assert!(scanner.has_cui_markings("UNCLASSIFIED//FOUO"));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_distribution_statements() {
        let scanner = DlpScanner::new();
        for letter in 'A'..='F' {
            let text = format!("Distribution {letter} -- approved for public release");
            assert!(
                scanner.has_cui_markings(&text),
                "Should detect Distribution {letter}"
            );
        }
    }

    // -- REQ-SECURITY-017: PII detection --

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_ssn() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("SSN: 123-45-6789");
        assert!(matches.iter().any(|m| m.category == DlpCategory::Ssn));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_email() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("Contact: user@example.com");
        assert!(matches.iter().any(|m| m.category == DlpCategory::Email));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_phone_number() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("Call (555) 123-4567");
        assert!(
            matches
                .iter()
                .any(|m| m.category == DlpCategory::PhoneNumber)
        );
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_credit_card_with_luhn() {
        let scanner = DlpScanner::new();
        // 4111111111111111 is a valid Luhn test number
        let matches = scanner.scan("Card: 4111 1111 1111 1111");
        assert!(
            matches
                .iter()
                .any(|m| m.category == DlpCategory::CreditCard),
            "Should detect valid Luhn credit card"
        );
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn rejects_invalid_luhn_credit_card() {
        let scanner = DlpScanner::new();
        // 4111111111111112 is NOT valid Luhn
        let matches = scanner.scan("Card: 4111 1111 1111 1112");
        assert!(
            !matches
                .iter()
                .any(|m| m.category == DlpCategory::CreditCard),
            "Should reject invalid Luhn number"
        );
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_ipv4_address() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("Server at 192.168.1.1");
        assert!(matches.iter().any(|m| m.category == DlpCategory::IpAddress));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn detects_api_key() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("api_key: sk_live_abcdefghij1234567890");
        assert!(matches.iter().any(|m| m.category == DlpCategory::ApiKey));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn has_pii_returns_true_for_ssn() {
        let scanner = DlpScanner::new();
        assert!(scanner.has_pii("SSN: 123-45-6789"));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn has_pii_returns_false_for_clean_text() {
        let scanner = DlpScanner::new();
        assert!(!scanner.has_pii("This is normal text with no PII"));
    }

    // -- False positive resistance --

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn normal_text_no_false_positives() {
        let scanner = DlpScanner::new();
        let text = "The quick brown fox jumps over the lazy dog. \
                    This is a normal paragraph about software development.";
        let matches = scanner.scan(text);
        assert!(
            matches.is_empty(),
            "Normal text should not trigger: {matches:?}"
        );
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn code_snippet_no_false_positives() {
        let scanner = DlpScanner::new();
        let text = "fn main() { println!(\"hello world\"); }";
        let matches = scanner.scan(text);
        assert!(matches.is_empty(), "Code should not trigger: {matches:?}");
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn line_number_is_correct() {
        let scanner = DlpScanner::new();
        let text = "line one\nline two\nSSN: 123-45-6789\nline four";
        let matches = scanner.scan(text);
        assert_eq!(matches[0].line_number, 3);
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn confidence_is_in_range() {
        let scanner = DlpScanner::new();
        let matches = scanner.scan("CUI//SP-CTI and SSN: 123-45-6789");
        for m in &matches {
            assert!(
                (0.0..=1.0).contains(&m.confidence),
                "confidence {} out of range",
                m.confidence
            );
        }
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn luhn_check_valid() {
        assert!(luhn_check("4111111111111111"));
        assert!(luhn_check("5500000000000004"));
    }

    // rtmx:req REQ-SECURITY-017
    #[test]
    fn luhn_check_invalid() {
        assert!(!luhn_check("4111111111111112"));
        assert!(!luhn_check("1234"));
    }
}
