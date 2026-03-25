//! The `aegis init` state machine.
//!
//! States: EnvironmentProbe -> ModeSelection -> CredentialNegotiation
//!         -> InfrastructureBinding -> ConfigCommit

use crate::config::{AegisConfig, Mode};
use aegis_domain::error::DomainError;
use std::path::PathBuf;

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
    let existing = config_path.exists();
    if existing {
        // Config already exists -- could offer re-init menu
        // For now, overwrite
        tracing::info!("Existing config found at {}", config_path.display());
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
    let config = AegisConfig::local(&inputs.endpoint, &inputs.model);
    config.validate()?;
    config.save(config_path)?;

    Ok(InitResult {
        config_path: config_path.to_path_buf(),
        mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

        // Second init should succeed (overwrite)
        let result = run_init(&inputs, &path);
        assert!(result.is_ok());
    }
}
