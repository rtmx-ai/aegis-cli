//! The `aegis init` state machine.
//!
//! States: EnvironmentProbe -> ModeSelection -> CredentialNegotiation
//!         -> InfrastructureBinding -> ConfigCommit

use crate::config::{AegisConfig, Mode, merge_config, preserves_audit_ledger};
use crate::tutorial;
use aegis_domain::error::DomainError;
use std::path::{Path, PathBuf};

/// States of the init state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitState {
    EnvironmentProbe,
    ModeSelection,
    CredentialNegotiation,
    InfrastructureBinding,
    ConfigCommit,
    Complete,
}

/// Result of the init process.
#[derive(Debug)]
pub struct InitResult {
    pub config_path: PathBuf,
    pub mode: Mode,
}

/// Inputs collected during the init flow.
pub struct InitInputs {
    pub mode: Mode,
    pub endpoint: String,
    pub model: String,
    pub region: Option<String>,
}

impl InitInputs {
    /// Create inputs for air-gapped local mode.
    pub fn local() -> Self {
        Self {
            mode: Mode::Local,
            endpoint: "http://localhost:11434/v1".to_string(),
            model: "llama3".to_string(),
            region: None,
        }
    }
}

/// Check whether the first-run wizard should be shown.
///
/// Returns `true` when no config file exists in the given directory,
/// meaning the user has never completed `aegis init`.
pub fn should_show_wizard(config_dir: &Path) -> bool {
    tutorial::is_first_run(config_dir)
}

/// Run the init state machine to completion.
///
/// For local mode, this skips credential negotiation and
/// infrastructure binding since no cloud access is needed.
pub fn run_init(
    inputs: &InitInputs,
    config_path: &std::path::Path,
) -> Result<InitResult, DomainError> {
    // State 0: Environment Probe
    // (Check for existing config, detect environment)
    let existing_config = if config_path.exists() {
        tracing::info!("Existing config found at {}", config_path.display());
        AegisConfig::load(config_path).ok()
    } else {
        None
    };

    // REQ-ONBOARD-005: Verify the audit ledger is not at risk.
    // The logs dir lives alongside the config file.
    if let Some(parent) = config_path.parent() {
        let logs_dir = parent.join("logs");
        if !preserves_audit_ledger(&logs_dir) {
            return Err(DomainError::ConfigError {
                message: format!(
                    "Audit ledger directory {} is missing or \
                     corrupted. Re-init aborted to prevent data loss.",
                    logs_dir.display()
                ),
            });
        }
    }

    // State 1: Mode Selection
    // (Already provided via inputs)
    let mode = inputs.mode.clone();

    // State 2: Credential Negotiation
    // (Skipped for local mode -- no cloud credentials needed)
    if mode != Mode::Local {
        // TODO: Cloud credential flows (ADC, mTLS, PKCE)
        return Err(DomainError::ConfigError {
            message: format!(
                "Mode {mode:?} not yet implemented. \
                 Use --local for air-gapped operation."
            ),
        });
    }

    // State 3: Infrastructure Binding
    // (Skipped for local mode -- no plugin invocation needed)

    // State 4: Config Commit
    // REQ-ONBOARD-004: Merge with existing config instead of
    // overwriting, so unchanged fields and infra outputs survive.
    let new_config = AegisConfig::local(&inputs.endpoint, &inputs.model);
    let final_config = match existing_config {
        Some(ref existing) => merge_config(existing, &new_config),
        None => new_config,
    };
    final_config.validate()?;
    final_config.save(config_path)?;

    Ok(InitResult {
        config_path: config_path.to_path_buf(),
        mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // @req REQ-ONBOARD-020
    #[test]
    fn should_show_wizard_true_when_no_config() {
        let tmp = TempDir::new().unwrap();
        assert!(
            should_show_wizard(tmp.path()),
            "Should show wizard when no config.yaml exists"
        );
    }

    // @req REQ-ONBOARD-020
    #[test]
    fn should_show_wizard_false_when_config_exists() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("config.yaml"), "version: '1.0'\n").unwrap();
        assert!(
            !should_show_wizard(tmp.path()),
            "Should NOT show wizard when config.yaml exists"
        );
    }

    // @req REQ-ONBOARD-020
    #[test]
    fn should_show_wizard_delegates_to_is_first_run() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            should_show_wizard(tmp.path()),
            crate::tutorial::is_first_run(tmp.path()),
            "should_show_wizard must agree with is_first_run"
        );
    }

    // @req REQ-ONBOARD-001
    #[test]
    fn init_state_machine_states_are_ordered() {
        let states = [
            InitState::EnvironmentProbe,
            InitState::ModeSelection,
            InitState::CredentialNegotiation,
            InitState::InfrastructureBinding,
            InitState::ConfigCommit,
            InitState::Complete,
        ];
        assert_eq!(states.len(), 6);
    }

    // @req REQ-ONBOARD-003
    #[test]
    fn init_local_creates_valid_config() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".aegis/config.yaml");

        let inputs = InitInputs::local();
        let result = run_init(&inputs, &config_path).unwrap();

        assert_eq!(result.mode, Mode::Local);
        assert!(result.config_path.exists());

        // Verify the config is loadable
        let config = AegisConfig::load(&config_path).unwrap();
        assert_eq!(config.mode, Mode::Local);
        assert_eq!(config.backend.provider, "local");
        assert_eq!(config.backend.endpoint, "http://localhost:11434/v1");
    }

    // @req REQ-ONBOARD-003
    #[test]
    fn init_local_makes_no_network_calls() {
        // Local init should succeed even with no network
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".aegis/config.yaml");

        let inputs = InitInputs::local();
        // This should complete without any network I/O
        let result = run_init(&inputs, &config_path);
        assert!(result.is_ok());
    }

    // @req REQ-ONBOARD-001
    #[test]
    fn init_cloud_modes_not_yet_implemented() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");

        let inputs = InitInputs {
            mode: Mode::SelfServiceByoc,
            endpoint: "https://vertex.googleapis.com".to_string(),
            model: "gemini-3.1-pro".to_string(),
            region: Some("us-central1".to_string()),
        };

        let result = run_init(&inputs, &path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not yet implemented"));
    }

    // @req REQ-ONBOARD-003
    #[test]
    fn init_local_default_inputs() {
        let inputs = InitInputs::local();
        assert_eq!(inputs.mode, Mode::Local);
        assert_eq!(inputs.endpoint, "http://localhost:11434/v1");
        assert_eq!(inputs.model, "llama3");
        assert!(inputs.region.is_none());
    }

    // @req REQ-ONBOARD-001
    #[test]
    fn init_detects_existing_config() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");

        // First init
        let inputs = InitInputs::local();
        run_init(&inputs, &path).unwrap();
        assert!(path.exists());

        // Second init should succeed (merge, not overwrite)
        let result = run_init(&inputs, &path);
        assert!(result.is_ok());
    }

    // @req REQ-ONBOARD-004
    #[test]
    fn reinit_preserves_infra_outputs() {
        use crate::config::toml_map::{InfraSection, PluginOutputs};
        use std::collections::HashMap;

        let tmp = TempDir::new().unwrap();
        let aegis_dir = tmp.path().join(".aegis");
        let config_path = aegis_dir.join("config.yaml");

        // First init
        let inputs = InitInputs::local();
        run_init(&inputs, &config_path).unwrap();

        // Manually inject infra outputs (simulating a plugin run)
        let mut config = AegisConfig::load(&config_path).unwrap();
        let mut plugins = HashMap::new();
        let mut outputs = HashMap::new();
        outputs.insert("project_id".to_string(), "aegis-il4-prod".to_string());
        plugins.insert(
            "gcp-assured-workloads".to_string(),
            PluginOutputs { outputs },
        );
        config.infra = InfraSection { plugins };
        config.save(&config_path).unwrap();

        // Re-init with a different model
        let new_inputs = InitInputs {
            mode: Mode::Local,
            endpoint: "http://localhost:11434/v1".to_string(),
            model: "mixtral-8x7b".to_string(),
            region: None,
        };
        run_init(&new_inputs, &config_path).unwrap();

        // Verify infra outputs survived
        let reloaded = AegisConfig::load(&config_path).unwrap();
        assert_eq!(reloaded.backend.model, "mixtral-8x7b");
        assert!(
            reloaded.infra.plugins.contains_key("gcp-assured-workloads"),
            "Plugin outputs must survive re-init"
        );
        assert_eq!(
            reloaded.infra.plugins["gcp-assured-workloads"].outputs["project_id"],
            "aegis-il4-prod"
        );
    }

    // @req REQ-ONBOARD-004
    #[test]
    fn reinit_updates_changed_fields() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.yaml");

        // First init with defaults
        let inputs = InitInputs::local();
        run_init(&inputs, &config_path).unwrap();

        let original = AegisConfig::load(&config_path).unwrap();
        assert_eq!(original.backend.model, "llama3");

        // Re-init with new model and endpoint
        let new_inputs = InitInputs {
            mode: Mode::Local,
            endpoint: "http://localhost:8080/v1".to_string(),
            model: "mixtral-8x7b".to_string(),
            region: None,
        };
        run_init(&new_inputs, &config_path).unwrap();

        let updated = AegisConfig::load(&config_path).unwrap();
        assert_eq!(updated.backend.model, "mixtral-8x7b");
        assert_eq!(updated.backend.endpoint, "http://localhost:8080/v1");
    }

    // @req REQ-ONBOARD-005
    #[test]
    fn reinit_preserves_audit_ledger_directory() {
        let tmp = TempDir::new().unwrap();
        let aegis_dir = tmp.path().join(".aegis");
        let config_path = aegis_dir.join("config.yaml");
        let logs_dir = aegis_dir.join("logs");

        // First init
        let inputs = InitInputs::local();
        run_init(&inputs, &config_path).unwrap();

        // Create the audit ledger directory with a sample file
        std::fs::create_dir_all(&logs_dir).unwrap();
        let ledger_file = logs_dir.join("session-001.jsonl");
        std::fs::write(&ledger_file, "{\"event\":\"start\"}\n").unwrap();

        // Re-init
        run_init(&inputs, &config_path).unwrap();

        // Verify audit ledger was not deleted or modified
        assert!(logs_dir.exists(), "Logs directory must survive");
        assert!(logs_dir.is_dir(), "Logs path must still be a dir");
        assert!(ledger_file.exists(), "Ledger file must survive re-init");
        let content = std::fs::read_to_string(&ledger_file).unwrap();
        assert_eq!(
            content, "{\"event\":\"start\"}\n",
            "Ledger content must be unchanged"
        );
    }

    // @req REQ-ONBOARD-005
    #[test]
    fn reinit_aborts_if_audit_ledger_corrupted() {
        let tmp = TempDir::new().unwrap();
        let aegis_dir = tmp.path().join(".aegis");
        let config_path = aegis_dir.join("config.yaml");
        let logs_path = aegis_dir.join("logs");

        // First init
        let inputs = InitInputs::local();
        run_init(&inputs, &config_path).unwrap();

        // Corrupt: replace logs dir with a file
        std::fs::create_dir_all(&aegis_dir).unwrap();
        std::fs::write(&logs_path, "not a directory").unwrap();

        // Re-init should fail
        let result = run_init(&inputs, &config_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Audit ledger directory"),
            "Error should mention audit ledger: {err}"
        );
    }
}
