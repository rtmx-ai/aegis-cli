//! Mock security filter for testing.

use aegis_domain::error::DomainError;
use aegis_domain::ports::SecurityFilter;
use aegis_domain::types::FilePath;

/// A mock filter that allows everything.
pub struct MockSecurityFilter;

impl SecurityFilter for MockSecurityFilter {
    fn is_blocked(&self, _path: &str) -> bool {
        false
    }

    fn validate_path(&self, path: &str) -> Result<FilePath, DomainError> {
        Ok(FilePath::new_unchecked(path))
    }
}
