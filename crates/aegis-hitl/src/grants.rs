//! Persistent session grants for time-limited permission overrides.
//!
//! Grants are serialized to JSON and stored alongside the session directory.
//! On load, expired grants are automatically filtered out.
//! rtmx:req REQ-HITL-008

use std::fs;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::rules::PermissionRule;

/// A time-limited permission grant (REQ-HITL-008).
///
/// Grants override the static rule set and trust level for the duration
/// of the grant. They are scoped to a session and tool/path combination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrant {
    /// The permission rule this grant overrides.
    pub rule: PermissionRule,
    /// When the grant was issued.
    pub granted_at: DateTime<Utc>,
    /// When the grant expires.
    pub expires_at: DateTime<Utc>,
    /// The session that issued this grant.
    pub session_id: String,
}

/// Default grant duration: 24 hours.
pub const DEFAULT_GRANT_DURATION_HOURS: i64 = 24;

impl PermissionGrant {
    /// Returns true if the grant has not yet expired.
    pub fn is_active(&self) -> bool {
        Utc::now() < self.expires_at
    }
}

/// Create a new grant with default 24h expiry.
pub fn create_grant(rule: PermissionRule, session_id: &str) -> PermissionGrant {
    let now = Utc::now();
    PermissionGrant {
        rule,
        granted_at: now,
        expires_at: now + Duration::hours(DEFAULT_GRANT_DURATION_HOURS),
        session_id: session_id.to_string(),
    }
}

/// Load grants from a JSON file, filtering out expired ones.
///
/// Returns an empty vec if the file does not exist.
pub fn load_grants(path: &Path) -> std::io::Result<Vec<PermissionGrant>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(path)?;
    let grants: Vec<PermissionGrant> = serde_json::from_str(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let now = Utc::now();
    Ok(grants.into_iter().filter(|g| g.expires_at > now).collect())
}

/// Save grants to a JSON file atomically (write to .tmp, then rename).
pub fn save_grants(path: &Path, grants: &[PermissionGrant]) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(grants)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, &json)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleEffect;

    fn sample_rule() -> PermissionRule {
        PermissionRule {
            tool: "write_file".to_string(),
            path_pattern: Some("src/**/*.rs".to_string()),
            effect: RuleEffect::Allow,
        }
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn grant_within_expiry_is_active() {
        let grant = create_grant(sample_rule(), "session-1");
        assert!(grant.is_active());
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn expired_grant_is_not_active() {
        let mut grant = create_grant(sample_rule(), "session-1");
        grant.expires_at = Utc::now() - Duration::hours(1);
        assert!(!grant.is_active());
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn prune_expired_removes_old_grants() {
        let active = create_grant(sample_rule(), "session-1");
        let mut expired = create_grant(sample_rule(), "session-2");
        expired.expires_at = Utc::now() - Duration::hours(1);

        let grants = vec![active.clone(), expired];
        let now = Utc::now();
        let pruned: Vec<_> = grants.into_iter().filter(|g| g.expires_at > now).collect();
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].session_id, "session-1");
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn grants_roundtrip_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grants.json");

        let grants = vec![
            create_grant(sample_rule(), "session-1"),
            create_grant(
                PermissionRule {
                    tool: "run_command".to_string(),
                    path_pattern: None,
                    effect: RuleEffect::Allow,
                },
                "session-2",
            ),
        ];

        save_grants(&path, &grants).unwrap();
        let loaded = load_grants(&path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].session_id, "session-1");
        assert_eq!(loaded[1].session_id, "session-2");
        assert_eq!(loaded[0].rule.tool, "write_file");
        assert_eq!(loaded[1].rule.tool, "run_command");
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn grants_atomic_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grants.json");

        let grants = vec![create_grant(sample_rule(), "session-1")];
        save_grants(&path, &grants).unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        // Verify it is valid JSON
        let parsed: Vec<PermissionGrant> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn create_grant_uses_24h_default() {
        let grant = create_grant(sample_rule(), "session-1");
        let expected_duration = Duration::hours(DEFAULT_GRANT_DURATION_HOURS);
        let actual_duration = grant.expires_at - grant.granted_at;
        // Allow 1 second tolerance for test execution time
        assert!((actual_duration - expected_duration).num_seconds().abs() < 2);
    }

    // rtmx:req REQ-HITL-008
    #[test]
    fn load_grants_returns_empty_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let grants = load_grants(&path).unwrap();
        assert!(grants.is_empty());
    }
}
