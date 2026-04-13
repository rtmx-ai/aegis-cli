//! Retention policy for audit log segments (REQ-AUDIT-010).
//!
//! Purges log segments older than a configurable age (default 90 days).
//! Handles both plain `.jsonl` and compressed `.jsonl.zst` files.

use std::io;
use std::path::{Path, PathBuf};

/// Default retention period in days.
pub const DEFAULT_RETENTION_DAYS: u64 = 90;

/// Retention policy that purges segments beyond a configurable age.
pub struct RetentionPolicy {
    pub max_age_days: u64,
}

impl RetentionPolicy {
    pub fn new(max_age_days: u64) -> Self {
        Self { max_age_days }
    }

    /// Purge segments older than `max_age_days`.
    ///
    /// Returns the count of removed files. Handles both `.jsonl` and
    /// `.jsonl.zst` files.
    pub fn enforce(&self, log_dir: &Path) -> io::Result<usize> {
        let expired = self.list_expired(log_dir)?;
        let count = expired.len();
        for path in &expired {
            std::fs::remove_file(path)?;
        }
        Ok(count)
    }

    /// List segments that would be purged (dry run).
    pub fn list_expired(&self, log_dir: &Path) -> io::Result<Vec<PathBuf>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.max_age_days as i64);
        let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

        let mut expired = Vec::new();
        let entries = std::fs::read_dir(log_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Only process aegis log files.
            if !name.starts_with("aegis-") {
                continue;
            }

            if let Some(date_str) = extract_date_from_filename(&name)
                && date_str < cutoff_date
            {
                expired.push(path);
            }
        }

        Ok(expired)
    }
}

/// Extract the YYYY-MM-DD date from a filename like
/// `aegis-2026-04-10.jsonl` or `aegis-2026-04-10.1.jsonl.zst`.
fn extract_date_from_filename(name: &str) -> Option<String> {
    // Strip "aegis-" prefix.
    let rest = name.strip_prefix("aegis-")?;
    // Date is always the first 10 chars: YYYY-MM-DD.
    if rest.len() < 10 {
        return None;
    }
    let date_part = &rest[..10];
    // Basic validation: must match YYYY-MM-DD pattern.
    if date_part.len() == 10 && date_part.as_bytes()[4] == b'-' && date_part.as_bytes()[7] == b'-'
    {
        Some(date_part.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_segment(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "{\"event\":\"test\"}\n").unwrap();
        path
    }

    // rtmx:req REQ-AUDIT-010
    #[test]
    fn enforce_removes_old_segments() {
        let tmp = TempDir::new().unwrap();
        let old = write_segment(tmp.path(), "aegis-2020-01-01.jsonl");

        let policy = RetentionPolicy::new(90);
        let count = policy.enforce(tmp.path()).unwrap();

        assert_eq!(count, 1);
        assert!(!old.exists(), "old segment should be removed");
    }

    // rtmx:req REQ-AUDIT-010
    #[test]
    fn enforce_keeps_recent_segments() {
        let tmp = TempDir::new().unwrap();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let recent = write_segment(tmp.path(), &format!("aegis-{today}.jsonl"));

        let policy = RetentionPolicy::new(90);
        let count = policy.enforce(tmp.path()).unwrap();

        assert_eq!(count, 0);
        assert!(recent.exists(), "recent segment should be kept");
    }

    // rtmx:req REQ-AUDIT-010
    #[test]
    fn enforce_handles_both_formats() {
        let tmp = TempDir::new().unwrap();
        let old_jsonl = write_segment(tmp.path(), "aegis-2020-01-01.jsonl");
        let old_zst = write_segment(tmp.path(), "aegis-2020-01-02.jsonl.zst");

        let policy = RetentionPolicy::new(90);
        let count = policy.enforce(tmp.path()).unwrap();

        assert_eq!(count, 2);
        assert!(!old_jsonl.exists());
        assert!(!old_zst.exists());
    }

    // rtmx:req REQ-AUDIT-010
    #[test]
    fn list_expired_dry_run() {
        let tmp = TempDir::new().unwrap();
        let old = write_segment(tmp.path(), "aegis-2020-01-01.jsonl");

        let policy = RetentionPolicy::new(90);
        let expired = policy.list_expired(tmp.path()).unwrap();

        assert_eq!(expired.len(), 1);
        // File should still exist (dry run).
        assert!(old.exists());
    }

    // rtmx:req REQ-AUDIT-010
    #[test]
    fn default_retention_is_90_days() {
        assert_eq!(DEFAULT_RETENTION_DAYS, 90);
    }
}
