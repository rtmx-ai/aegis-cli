//! Plugin host: spawn, communicate, and manage plugin subprocesses.

use crate::events::*;
use aegis_domain::error::DomainError;
use std::path::{Path, PathBuf};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;

/// A registered plugin with its binary path and manifest.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub binary: PathBuf,
    pub manifest: PluginManifest,
}

/// Result of running a plugin subcommand.
#[derive(Debug)]
pub struct PluginOutput {
    pub events: Vec<PluginEvent>,
    pub result: Option<ResultEvent>,
    pub stderr: String,
    pub exit_code: i32,
}

/// Discover a plugin by invoking its `manifest` subcommand.
pub async fn discover_plugin(binary: &Path) -> Result<Plugin, DomainError> {
    let output = Command::new(binary)
        .arg("manifest")
        .output()
        .await
        .map_err(|e| DomainError::ProviderError {
            message: format!("Failed to run plugin {}: {e}", binary.display()),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DomainError::ProviderError {
            message: format!(
                "Plugin manifest failed (exit {}): {}",
                output.status, stderr
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let manifest =
        parse_manifest(stdout.trim()).map_err(|e| DomainError::ProviderError { message: e })?;

    if manifest.contract != "aegis-infra/v1" {
        return Err(DomainError::ProviderError {
            message: format!(
                "Incompatible protocol: expected aegis-infra/v1, got {}",
                manifest.contract
            ),
        });
    }

    Ok(Plugin {
        binary: binary.to_path_buf(),
        manifest,
    })
}

/// Run a plugin subcommand and collect all NDJSON events.
pub async fn run_plugin(
    plugin: &Plugin,
    subcommand: &str,
    input_json: Option<&str>,
    timeout_secs: u64,
) -> Result<PluginOutput, DomainError> {
    let mut cmd = Command::new(&plugin.binary);
    cmd.arg(subcommand);

    if let Some(input) = input_json {
        cmd.arg("--input").arg(input);
    }

    if subcommand == "destroy" {
        cmd.arg("--confirm-destroy");
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| DomainError::ProviderError {
        message: format!("Failed to spawn plugin {}: {e}", plugin.manifest.name),
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DomainError::ProviderError {
            message: "Failed to capture plugin stdout".to_string(),
        })?;

    // Parse NDJSON events from stdout
    let mut events = Vec::new();
    let mut result_event = None;
    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();

    let parse_future = async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(event) = parse_event(&line) {
                if let PluginEvent::Result(ref r) = event {
                    result_event = Some(r.clone());
                }
                events.push(event);
            } else if !line.trim().is_empty() {
                tracing::warn!("Unparseable plugin output: {line}");
            }
        }
    };

    // Apply timeout
    let timed_out =
        tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), parse_future)
            .await
            .is_err();

    if timed_out {
        let _ = child.kill().await;
        return Err(DomainError::ProviderError {
            message: format!(
                "Plugin {} timed out after {timeout_secs}s",
                plugin.manifest.name
            ),
        });
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| DomainError::ProviderError {
            message: format!("Plugin wait failed: {e}"),
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    if !output.status.success() && result_event.is_none() {
        return Err(DomainError::ProviderError {
            message: format!(
                "Plugin {} failed (exit {exit_code}): {stderr}",
                plugin.manifest.name
            ),
        });
    }

    Ok(PluginOutput {
        events,
        result: result_event,
        stderr,
        exit_code,
    })
}

/// Aggregate health check results from check events.
pub fn aggregate_health(checks: &[CheckEvent]) -> (bool, String) {
    let total = checks.len();
    let passed = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Pass)
        .count();
    let warned = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Warn)
        .count();
    let failed = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Fail)
        .count();

    let success = failed == 0;
    let summary = if warned > 0 {
        format!("{passed} passed, {warned} warned, {failed} failed ({total} total)")
    } else {
        format!("{passed} passed, {failed} failed ({total} total)")
    };

    (success, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_mock_plugin(dir: &Path, name: &str, script: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    // @req REQ-INFRA-001
    #[tokio::test]
    async fn discover_valid_plugin() {
        let tmp = TempDir::new().unwrap();
        let script = r#"#!/bin/sh
echo '{"name":"test-plugin","version":"0.1.0","contract":"aegis-infra/v1","description":"Test"}'
"#;
        let bin = write_mock_plugin(tmp.path(), "test-plugin", script);
        let plugin = discover_plugin(&bin).await.unwrap();
        assert_eq!(plugin.manifest.name, "test-plugin");
        assert_eq!(plugin.manifest.contract, "aegis-infra/v1");
    }

    // @req REQ-INFRA-002
    #[tokio::test]
    async fn discover_rejects_incompatible_protocol() {
        let tmp = TempDir::new().unwrap();
        let script = r#"#!/bin/sh
echo '{"name":"old-plugin","version":"0.1.0","contract":"aegis-infra/v99"}'
"#;
        let bin = write_mock_plugin(tmp.path(), "old-plugin", script);
        let result = discover_plugin(&bin).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Incompatible protocol"),
            "Should reject incompatible protocol: {err}"
        );
    }

    // @req REQ-INFRA-006
    #[tokio::test]
    async fn discover_handles_failed_plugin() {
        let tmp = TempDir::new().unwrap();
        let script = "#!/bin/sh\nexit 1\n";
        let bin = write_mock_plugin(tmp.path(), "bad-plugin", script);
        let result = discover_plugin(&bin).await;
        assert!(result.is_err());
    }

    // @req REQ-INFRA-001
    #[tokio::test]
    async fn run_plugin_collects_ndjson_events() {
        let tmp = TempDir::new().unwrap();
        let script = r#"#!/bin/sh
echo '{"type":"diagnostic","severity":"info","message":"Starting"}'
echo '{"type":"progress","resource":"kms","operation":"create","status":"complete"}'
echo '{"type":"check","name":"kms_key","status":"pass","detail":"OK"}'
echo '{"type":"result","success":true,"outputs":{"key":"value"}}'
"#;
        let bin = write_mock_plugin(tmp.path(), "event-plugin", script);
        let plugin = Plugin {
            binary: bin,
            manifest: PluginManifest {
                name: "event-plugin".to_string(),
                version: "0.1.0".to_string(),
                contract: "aegis-infra/v1".to_string(),
                description: None,
            },
        };

        let output = run_plugin(&plugin, "status", None, 10).await.unwrap();

        assert_eq!(output.events.len(), 4);
        assert!(output.result.is_some());
        assert!(output.result.unwrap().success);
        assert_eq!(output.exit_code, 0);
    }

    // @req REQ-INFRA-006
    #[tokio::test]
    async fn run_plugin_captures_stderr_on_failure() {
        let tmp = TempDir::new().unwrap();
        let script = "#!/bin/sh\necho 'quota exceeded' >&2\nexit 2\n";
        let bin = write_mock_plugin(tmp.path(), "fail-plugin", script);
        let plugin = Plugin {
            binary: bin,
            manifest: PluginManifest {
                name: "fail-plugin".to_string(),
                version: "0.1.0".to_string(),
                contract: "aegis-infra/v1".to_string(),
                description: None,
            },
        };

        let result = run_plugin(&plugin, "up", Some("{}"), 10).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("quota exceeded"),
            "Should contain stderr: {err}"
        );
    }

    // @req REQ-INFRA-004
    #[tokio::test]
    async fn run_plugin_skips_malformed_lines() {
        let tmp = TempDir::new().unwrap();
        let script = r#"#!/bin/sh
echo 'not json'
echo '{"type":"diagnostic","severity":"info","message":"OK"}'
echo ''
echo '{"type":"result","success":true}'
"#;
        let bin = write_mock_plugin(tmp.path(), "messy-plugin", script);
        let plugin = Plugin {
            binary: bin,
            manifest: PluginManifest {
                name: "messy-plugin".to_string(),
                version: "0.1.0".to_string(),
                contract: "aegis-infra/v1".to_string(),
                description: None,
            },
        };

        let output = run_plugin(&plugin, "status", None, 10).await.unwrap();

        // Only the 2 valid JSON lines should be parsed
        assert_eq!(output.events.len(), 2);
    }

    // @req REQ-INFRA-010
    #[test]
    fn aggregate_health_all_pass() {
        let checks = vec![
            CheckEvent {
                name: "kms".to_string(),
                status: CheckStatus::Pass,
                detail: None,
            },
            CheckEvent {
                name: "vpc".to_string(),
                status: CheckStatus::Pass,
                detail: None,
            },
        ];
        let (success, summary) = aggregate_health(&checks);
        assert!(success);
        assert!(summary.contains("2 passed"));
    }

    // @req REQ-INFRA-010
    #[test]
    fn aggregate_health_with_failure() {
        let checks = vec![
            CheckEvent {
                name: "kms".to_string(),
                status: CheckStatus::Pass,
                detail: None,
            },
            CheckEvent {
                name: "vpc".to_string(),
                status: CheckStatus::Fail,
                detail: Some("Perimeter inactive".to_string()),
            },
        ];
        let (success, summary) = aggregate_health(&checks);
        assert!(!success);
        assert!(summary.contains("1 failed"));
    }

    // @req REQ-INFRA-010
    #[test]
    fn aggregate_health_with_warn() {
        let checks = vec![
            CheckEvent {
                name: "kms".to_string(),
                status: CheckStatus::Pass,
                detail: None,
            },
            CheckEvent {
                name: "audit".to_string(),
                status: CheckStatus::Warn,
                detail: Some("Permission denied".to_string()),
            },
        ];
        let (success, summary) = aggregate_health(&checks);
        assert!(success, "Warns should not fail");
        assert!(summary.contains("1 warned"));
    }
}
