//! Automatic config schema migration (REQ-ONBOARD-010).
//!
//! On config load, if `schema_version` < `CURRENT_SCHEMA_VERSION`, applies
//! a chain of migration functions. Backs up original before migrating.
//! Fails loudly rather than silently corrupting config.

use std::path::Path;
use tracing::info;

/// The current schema version that new configs are created at.
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// Migrate a config file at `path` to the current schema version.
///
/// 1. Reads raw YAML.
/// 2. Checks `schema_version` field (defaults to 1 if absent).
/// 3. If version < current, backs up to `config.yaml.v{old}.bak`.
/// 4. Applies migration chain (v1->v2, v2->v3, etc.).
/// 5. Writes updated config back.
/// 6. Returns the migrated config.
pub fn migrate_config(path: &Path) -> Result<crate::config::AegisConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    let mut value: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("Invalid YAML: {e}"))?;

    let version = extract_schema_version(&value);

    if version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "Config schema_version {} is newer than supported version {}. \
             Please upgrade aegis.",
            version, CURRENT_SCHEMA_VERSION
        ));
    }

    if version < CURRENT_SCHEMA_VERSION {
        // Back up the original file.
        let backup_path = path.with_extension(format!("yaml.v{version}.bak"));
        std::fs::copy(path, &backup_path)
            .map_err(|e| format!("Failed to back up config to {}: {e}", backup_path.display()))?;
        info!(
            old_version = version,
            new_version = CURRENT_SCHEMA_VERSION,
            backup = %backup_path.display(),
            "Migrating config schema"
        );

        // Apply migration chain.
        let mut current = version;
        while current < CURRENT_SCHEMA_VERSION {
            match current {
                1 => migrate_v1_to_v2(&mut value),
                other => {
                    return Err(format!(
                        "No migration defined from v{other} to v{}",
                        other + 1
                    ));
                }
            }
            current += 1;
        }

        // Write migrated config back.
        let yaml = serde_yaml::to_string(&value)
            .map_err(|e| format!("Failed to serialize migrated config: {e}"))?;
        std::fs::write(path, &yaml)
            .map_err(|e| format!("Failed to write migrated config: {e}"))?;
    }

    // Parse the (possibly migrated) value into AegisConfig.
    serde_yaml::from_value(value).map_err(|e| format!("Failed to parse migrated config: {e}"))
}

/// Extract the schema_version from a YAML value, defaulting to 1.
fn extract_schema_version(value: &serde_yaml::Value) -> u32 {
    value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(1)
}

/// Migration from v1 to v2:
/// - Adds `schema_version: 2`
/// - Adds `feedback` section with defaults if missing
fn migrate_v1_to_v2(value: &mut serde_yaml::Value) {
    if let serde_yaml::Value::Mapping(map) = value {
        // Set schema_version to 2.
        map.insert(
            serde_yaml::Value::String("schema_version".to_string()),
            serde_yaml::Value::Number(serde_yaml::Number::from(2u64)),
        );

        // Add feedback section with defaults if missing.
        let feedback_key = serde_yaml::Value::String("feedback".to_string());
        if !map.contains_key(&feedback_key) {
            let mut feedback = serde_yaml::Mapping::new();
            feedback.insert(
                serde_yaml::Value::String("prompt_after_sessions".to_string()),
                serde_yaml::Value::Number(serde_yaml::Number::from(10u64)),
            );
            feedback.insert(
                serde_yaml::Value::String("session_count".to_string()),
                serde_yaml::Value::Number(serde_yaml::Number::from(0u64)),
            );
            feedback.insert(
                serde_yaml::Value::String("feedback_prompted".to_string()),
                serde_yaml::Value::Bool(false),
            );
            map.insert(feedback_key, serde_yaml::Value::Mapping(feedback));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_yaml(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("config.yaml");
        std::fs::write(&path, content).unwrap();
        path
    }

    // rtmx:req REQ-ONBOARD-010
    #[test]
    fn test_config_migration_v1_to_v2() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
version: "1.0"
mode: local
backend:
  provider: local
  model: llama3
  endpoint: http://localhost:11434/v1
  max_tokens: 4096
"#;
        let path = write_yaml(tmp.path(), yaml);
        let config = migrate_config(&path).unwrap();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    }

    // rtmx:req REQ-ONBOARD-010
    #[test]
    fn test_migration_creates_backup() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
version: "1.0"
mode: local
backend:
  provider: local
  model: llama3
  endpoint: http://localhost:11434/v1
  max_tokens: 4096
"#;
        let path = write_yaml(tmp.path(), yaml);
        migrate_config(&path).unwrap();
        let backup = tmp.path().join("config.yaml.v1.bak");
        assert!(backup.exists(), "Backup file should exist after migration");
    }

    // rtmx:req REQ-ONBOARD-010
    #[test]
    fn test_migration_skips_current_version() {
        let tmp = TempDir::new().unwrap();
        let yaml = format!(
            r#"
schema_version: {CURRENT_SCHEMA_VERSION}
version: "1.0"
mode: local
backend:
  provider: local
  model: llama3
  endpoint: http://localhost:11434/v1
  max_tokens: 4096
"#,
        );
        let path = write_yaml(tmp.path(), &yaml);
        let config = migrate_config(&path).unwrap();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        // No backup should have been created.
        let backup = tmp.path().join("config.yaml.v2.bak");
        assert!(
            !backup.exists(),
            "No backup when already at current version"
        );
    }

    // rtmx:req REQ-ONBOARD-010
    #[test]
    fn test_migration_fails_on_future_version() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
schema_version: 99
version: "1.0"
mode: local
backend:
  provider: local
  model: llama3
  endpoint: http://localhost:11434/v1
  max_tokens: 4096
"#;
        let path = write_yaml(tmp.path(), yaml);
        let result = migrate_config(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("newer than supported"));
    }

    // rtmx:req REQ-ONBOARD-010
    #[test]
    fn test_migration_preserves_existing_fields() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
version: "1.0"
mode: enterprise-byoc
backend:
  provider: vertex-ai
  model: gemini-pro
  endpoint: https://vertex:443/v1
  region: us-central1
  max_tokens: 8192
"#;
        let path = write_yaml(tmp.path(), yaml);
        let config = migrate_config(&path).unwrap();
        assert_eq!(config.mode, crate::config::Mode::EnterpriseByoc);
        assert_eq!(config.backend.provider, "vertex-ai");
        assert_eq!(config.backend.model, "gemini-pro");
        assert_eq!(config.backend.endpoint, "https://vertex:443/v1");
        assert_eq!(config.backend.region, Some("us-central1".to_string()));
        assert_eq!(config.backend.max_tokens, 8192);
    }
}
