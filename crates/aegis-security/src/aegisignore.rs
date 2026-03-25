//! .aegisignore context filtering with mandatory blocklist.

use aegis_domain::error::DomainError;
use aegis_domain::ports::SecurityFilter;
use aegis_domain::types::FilePath;

/// Default mandatory blocklist patterns.
const MANDATORY_BLOCKLIST: &[&str] = &[
    ".env",
    "*.pem",
    "*.key",
    "*.pfx",
    "*.p12",
    "**/credentials",
    "**/credentials.json",
    "**/.aws/credentials",
    "**/.ssh/id_*",
];

pub struct AegisIgnore {
    patterns: Vec<String>,
}

impl AegisIgnore {
    /// Create a filter with only the mandatory blocklist.
    pub fn with_defaults() -> Self {
        Self {
            patterns: MANDATORY_BLOCKLIST.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Check if a path matches any blocklist pattern.
    fn matches_pattern(&self, path: &str, pattern: &str) -> bool {
        // Simple glob matching for mandatory blocklist patterns.
        // Supports: **/prefix*, *.ext, **/literal, and exact matches.
        if pattern.starts_with("**/") {
            let suffix = &pattern[3..];
            if suffix.ends_with('*') {
                // **/prefix* -- match any path segment containing the prefix
                let prefix = &suffix[..suffix.len() - 1];
                path.contains(prefix)
            } else {
                path.contains(suffix)
            }
        } else if pattern.starts_with("*.") {
            path.ends_with(&pattern[1..])
        } else {
            path == pattern || path.ends_with(&format!("/{}", pattern))
        }
    }
}

impl SecurityFilter for AegisIgnore {
    fn is_blocked(&self, path: &str) -> bool {
        self.patterns.iter().any(|p| self.matches_pattern(path, p))
    }

    fn validate_path(&self, path: &str) -> Result<FilePath, DomainError> {
        if self.is_blocked(path) {
            Err(DomainError::FileBlocked {
                path: path.to_string(),
            })
        } else {
            Ok(FilePath::new_unchecked(path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    // @req REQ-SECURITY-001
    #[rstest]
    #[case(".env", true)]
    #[case("config/.env", true)]
    #[case("secrets/server.pem", true)]
    #[case("tls/cert.key", true)]
    #[case("home/.aws/credentials", true)]
    #[case("users/.ssh/id_rsa", true)]
    #[case("src/main.rs", false)]
    #[case("README.md", false)]
    #[case("Cargo.toml", false)]
    #[case("src/env.rs", false)]
    fn mandatory_blocklist_enforcement(#[case] path: &str, #[case] blocked: bool) {
        let filter = AegisIgnore::with_defaults();
        assert_eq!(filter.is_blocked(path), blocked, "path: {}", path);
    }

    // @req REQ-SECURITY-001
    #[test]
    fn validate_path_returns_error_for_blocked() {
        let filter = AegisIgnore::with_defaults();
        assert!(filter.validate_path(".env").is_err());
        assert!(filter.validate_path("src/main.rs").is_ok());
    }
}
