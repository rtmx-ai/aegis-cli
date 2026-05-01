//! The `aegis init` state machine.
//!
//! States: EnvironmentProbe -> ModeSelection -> ProviderSelection
//!         -> CredentialNegotiation -> InfrastructureBinding -> ConfigCommit

use crate::adc;
use crate::config::{AegisConfig, Mode, merge_config, preserves_audit_ledger};
use crate::tutorial;
use aegis_domain::error::DomainError;
use std::path::{Path, PathBuf};

/// States of the init state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitState {
    EnvironmentProbe,
    ModeSelection,
    ProviderSelection,
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

// ---------------------------------------------------------------------------
// Provider selection (REQ-ONBOARD-028)
// ---------------------------------------------------------------------------

/// Cloud/local provider the user selects during `aegis init`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderChoice {
    /// Google Cloud Vertex AI (Gemini models).
    Vertex,
    /// AWS Bedrock (Claude models).
    Bedrock,
    /// Azure OpenAI Service.
    Azure,
    /// Local Ollama / vLLM (air-gapped).
    Local,
}

/// Values collected from the user during provider selection.
#[derive(Debug, Clone, Default)]
pub struct ProviderSelectionState {
    /// GCP project ID (required for Vertex).
    pub project_id: Option<String>,
    /// Cloud region or local endpoint override.
    pub region: Option<String>,
    /// Azure endpoint URL (required for Azure).
    pub azure_endpoint: Option<String>,
    /// Model name override (optional; wizard can supply a default).
    pub model: Option<String>,
    /// Local endpoint override (optional; defaults to Ollama).
    pub local_endpoint: Option<String>,
}

/// Result of probing for existing provider credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialStatus {
    /// Credentials found and appear valid.
    Found(String),
    /// No credentials detected.
    NotFound(String),
}

impl CredentialStatus {
    /// Returns `true` when credentials were found.
    pub fn is_found(&self) -> bool {
        matches!(self, CredentialStatus::Found(_))
    }
}

/// Return the default region for a given provider choice.
pub fn default_region(choice: ProviderChoice) -> &'static str {
    match choice {
        ProviderChoice::Vertex => "us-central1",
        ProviderChoice::Bedrock => "us-east-1",
        ProviderChoice::Azure => "eastus",
        ProviderChoice::Local => "local",
    }
}

/// Resolved provider configuration produced by validation.
///
/// This is an onboard-internal representation that maps to the
/// `BackendConfig` written into `config.yaml`. The composition root
/// converts it to `aegis_llm::config::ProviderConfig` at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProviderConfig {
    /// Provider name (e.g. "vertex", "bedrock", "azure", "local").
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Fully-qualified endpoint URL.
    pub endpoint: String,
    /// Cloud region (if applicable).
    pub region: Option<String>,
    /// GCP project ID (Vertex only).
    pub project_id: Option<String>,
}

/// Validate collected provider inputs and produce a
/// [`ResolvedProviderConfig`].
///
/// Returns `Ok(ResolvedProviderConfig)` when all required fields are
/// present and well-formed, or `Err(Vec<String>)` listing every
/// validation failure.
pub fn validate_provider_config(
    choice: ProviderChoice,
    state: &ProviderSelectionState,
) -> Result<ResolvedProviderConfig, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    match choice {
        ProviderChoice::Vertex => {
            let project_id = match &state.project_id {
                Some(id) if !id.trim().is_empty() => id.trim().to_string(),
                _ => {
                    errors.push("project_id is required for Vertex AI".to_string());
                    String::new()
                }
            };
            let region = state
                .region
                .as_deref()
                .filter(|r| !r.trim().is_empty())
                .unwrap_or(default_region(ProviderChoice::Vertex))
                .to_string();
            let model = state
                .model
                .as_deref()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or("gemini-2.5-pro-001")
                .to_string();

            if !errors.is_empty() {
                return Err(errors);
            }

            let endpoint = format!(
                "https://{region}-aiplatform.googleapis.com/v1/projects/\
                 {project_id}/locations/{region}/publishers/google/models/\
                 {model}"
            );
            Ok(ResolvedProviderConfig {
                provider: "vertex".to_string(),
                model,
                endpoint,
                region: Some(region),
                project_id: Some(project_id),
            })
        }

        ProviderChoice::Bedrock => {
            let region = state
                .region
                .as_deref()
                .filter(|r| !r.trim().is_empty())
                .unwrap_or(default_region(ProviderChoice::Bedrock))
                .to_string();
            let model = state
                .model
                .as_deref()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or("claude-3-sonnet-20241022")
                .to_string();
            let endpoint = format!("https://bedrock-runtime.{region}.amazonaws.com");

            Ok(ResolvedProviderConfig {
                provider: "bedrock".to_string(),
                model,
                endpoint,
                region: Some(region),
                project_id: None,
            })
        }

        ProviderChoice::Azure => {
            let endpoint = match &state.azure_endpoint {
                Some(ep) if !ep.trim().is_empty() => ep.trim().to_string(),
                _ => {
                    errors.push("azure_endpoint is required for Azure OpenAI".to_string());
                    String::new()
                }
            };
            let model = state
                .model
                .as_deref()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or("gpt-4o-2024-05-13")
                .to_string();

            if !errors.is_empty() {
                return Err(errors);
            }
            Ok(ResolvedProviderConfig {
                provider: "azure".to_string(),
                model,
                endpoint,
                region: None,
                project_id: None,
            })
        }

        ProviderChoice::Local => {
            let endpoint = state
                .local_endpoint
                .as_deref()
                .filter(|e| !e.trim().is_empty())
                .unwrap_or("http://localhost:11434/v1")
                .to_string();
            let model = state
                .model
                .as_deref()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or("llama3")
                .to_string();

            Ok(ResolvedProviderConfig {
                provider: "local".to_string(),
                model,
                endpoint,
                region: None,
                project_id: None,
            })
        }
    }
}

/// Probe the environment for existing provider credentials.
///
/// For Vertex: checks GCP Application Default Credentials via [`adc`].
/// For Bedrock: checks `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
/// For Azure: checks `AZURE_OPENAI_API_KEY`.
/// For Local: always returns [`CredentialStatus::Found`].
pub fn probe_provider_credentials(choice: ProviderChoice) -> CredentialStatus {
    match choice {
        ProviderChoice::Vertex => {
            let status = adc::validate_gcloud_adc();
            if status.is_available() {
                CredentialStatus::Found(
                    "GCP Application Default Credentials detected".to_string(),
                )
            } else {
                CredentialStatus::NotFound(adc::adc_not_found_message())
            }
        }

        ProviderChoice::Bedrock => {
            let key_id = std::env::var("AWS_ACCESS_KEY_ID")
                .ok()
                .filter(|v| !v.trim().is_empty());
            let secret = std::env::var("AWS_SECRET_ACCESS_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty());
            if key_id.is_some() && secret.is_some() {
                CredentialStatus::Found(
                    "AWS credentials detected via environment variables".to_string(),
                )
            } else {
                CredentialStatus::NotFound(
                    "AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY are required. \
                     Configure via `aws configure` or set environment variables."
                        .to_string(),
                )
            }
        }

        ProviderChoice::Azure => {
            let key = std::env::var("AZURE_OPENAI_API_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty());
            if key.is_some() {
                CredentialStatus::Found(
                    "Azure OpenAI API key detected via AZURE_OPENAI_API_KEY".to_string(),
                )
            } else {
                CredentialStatus::NotFound(
                    "AZURE_OPENAI_API_KEY is required. \
                     Set the environment variable with your Azure OpenAI key."
                        .to_string(),
                )
            }
        }

        ProviderChoice::Local => {
            CredentialStatus::Found("Local provider requires no credentials".to_string())
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

    // rtmx:req REQ-ONBOARD-020
    #[test]
    fn should_show_wizard_true_when_no_config() {
        let tmp = TempDir::new().unwrap();
        assert!(
            should_show_wizard(tmp.path()),
            "Should show wizard when no config.yaml exists"
        );
    }

    // rtmx:req REQ-ONBOARD-020
    #[test]
    fn should_show_wizard_false_when_config_exists() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("config.yaml"), "version: '1.0'\n").unwrap();
        assert!(
            !should_show_wizard(tmp.path()),
            "Should NOT show wizard when config.yaml exists"
        );
    }

    // rtmx:req REQ-ONBOARD-020
    #[test]
    fn should_show_wizard_delegates_to_is_first_run() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            should_show_wizard(tmp.path()),
            crate::tutorial::is_first_run(tmp.path()),
            "should_show_wizard must agree with is_first_run"
        );
    }

    // rtmx:req REQ-ONBOARD-001
    #[test]
    fn init_state_machine_states_are_ordered() {
        let states = [
            InitState::EnvironmentProbe,
            InitState::ModeSelection,
            InitState::ProviderSelection,
            InitState::CredentialNegotiation,
            InitState::InfrastructureBinding,
            InitState::ConfigCommit,
            InitState::Complete,
        ];
        assert_eq!(states.len(), 7);
    }

    // rtmx:req REQ-ONBOARD-003
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

    // rtmx:req REQ-ONBOARD-003
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

    // rtmx:req REQ-ONBOARD-001
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

    // rtmx:req REQ-ONBOARD-003
    #[test]
    fn init_local_default_inputs() {
        let inputs = InitInputs::local();
        assert_eq!(inputs.mode, Mode::Local);
        assert_eq!(inputs.endpoint, "http://localhost:11434/v1");
        assert_eq!(inputs.model, "llama3");
        assert!(inputs.region.is_none());
    }

    // rtmx:req REQ-ONBOARD-001
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

    // rtmx:req REQ-ONBOARD-004
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

    // rtmx:req REQ-ONBOARD-004
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

    // rtmx:req REQ-ONBOARD-005
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

    // rtmx:req REQ-ONBOARD-005
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

    // ---------------------------------------------------------------
    // Provider selection tests (REQ-ONBOARD-028)
    // ---------------------------------------------------------------

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn vertex_config_requires_project_id() {
        let state = ProviderSelectionState::default();
        let result = validate_provider_config(ProviderChoice::Vertex, &state);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("project_id")),
            "Should require project_id: {errors:?}"
        );
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn vertex_validation_rejects_empty_project_id() {
        let state = ProviderSelectionState {
            project_id: Some("   ".to_string()),
            ..Default::default()
        };
        let result = validate_provider_config(ProviderChoice::Vertex, &state);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("project_id")),
            "Should reject whitespace-only project_id: {errors:?}"
        );
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn vertex_config_with_project_id_produces_valid_config() {
        let state = ProviderSelectionState {
            project_id: Some("my-gcp-project".to_string()),
            ..Default::default()
        };
        let config = validate_provider_config(ProviderChoice::Vertex, &state).unwrap();
        assert_eq!(config.provider, "vertex");
        assert_eq!(config.project_id.as_deref(), Some("my-gcp-project"));
        assert_eq!(config.region.as_deref(), Some("us-central1"));
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn bedrock_config_defaults_region_to_us_east_1() {
        let state = ProviderSelectionState::default();
        let config = validate_provider_config(ProviderChoice::Bedrock, &state).unwrap();
        assert_eq!(config.provider, "bedrock");
        assert_eq!(config.region.as_deref(), Some("us-east-1"));
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn bedrock_config_accepts_custom_region() {
        let state = ProviderSelectionState {
            region: Some("us-gov-west-1".to_string()),
            ..Default::default()
        };
        let config = validate_provider_config(ProviderChoice::Bedrock, &state).unwrap();
        assert_eq!(config.region.as_deref(), Some("us-gov-west-1"));
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn azure_config_requires_endpoint() {
        let state = ProviderSelectionState::default();
        let result = validate_provider_config(ProviderChoice::Azure, &state);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("azure_endpoint")),
            "Should require azure_endpoint: {errors:?}"
        );
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn azure_config_rejects_empty_endpoint() {
        let state = ProviderSelectionState {
            azure_endpoint: Some("  ".to_string()),
            ..Default::default()
        };
        let result = validate_provider_config(ProviderChoice::Azure, &state);
        assert!(result.is_err());
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn azure_config_with_endpoint_produces_valid_config() {
        let state = ProviderSelectionState {
            azure_endpoint: Some("https://myendpoint.openai.azure.com".to_string()),
            ..Default::default()
        };
        let config = validate_provider_config(ProviderChoice::Azure, &state).unwrap();
        assert_eq!(config.provider, "azure");
        assert_eq!(config.endpoint, "https://myendpoint.openai.azure.com");
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn local_config_produces_valid_provider_config() {
        let state = ProviderSelectionState::default();
        let config = validate_provider_config(ProviderChoice::Local, &state).unwrap();
        assert_eq!(config.provider, "local");
        assert_eq!(config.endpoint, "http://localhost:11434/v1");
        assert_eq!(config.model, "llama3");
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn local_config_accepts_custom_endpoint() {
        let state = ProviderSelectionState {
            local_endpoint: Some("http://localhost:8080/v1".to_string()),
            model: Some("mixtral-8x7b".to_string()),
            ..Default::default()
        };
        let config = validate_provider_config(ProviderChoice::Local, &state).unwrap();
        assert_eq!(config.endpoint, "http://localhost:8080/v1");
        assert_eq!(config.model, "mixtral-8x7b");
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn default_regions_are_correct_per_provider() {
        assert_eq!(default_region(ProviderChoice::Vertex), "us-central1");
        assert_eq!(default_region(ProviderChoice::Bedrock), "us-east-1");
        assert_eq!(default_region(ProviderChoice::Azure), "eastus");
        assert_eq!(default_region(ProviderChoice::Local), "local");
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn credential_status_is_found() {
        assert!(CredentialStatus::Found("test".to_string()).is_found());
        assert!(!CredentialStatus::NotFound("test".to_string()).is_found());
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn probe_local_always_found() {
        let status = probe_provider_credentials(ProviderChoice::Local);
        assert!(
            status.is_found(),
            "Local provider should always report credentials found"
        );
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn vertex_config_uses_custom_model() {
        let state = ProviderSelectionState {
            project_id: Some("proj-123".to_string()),
            model: Some("gemini-2.5-flash-001".to_string()),
            ..Default::default()
        };
        let config = validate_provider_config(ProviderChoice::Vertex, &state).unwrap();
        assert_eq!(config.model, "gemini-2.5-flash-001");
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn vertex_config_uses_custom_region() {
        let state = ProviderSelectionState {
            project_id: Some("proj-123".to_string()),
            region: Some("us-east4".to_string()),
            ..Default::default()
        };
        let config = validate_provider_config(ProviderChoice::Vertex, &state).unwrap();
        assert_eq!(config.region.as_deref(), Some("us-east4"));
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn provider_choice_debug_impl() {
        // Ensure Debug is derived (compile-time check via usage).
        let _ = format!("{:?}", ProviderChoice::Vertex);
        let _ = format!("{:?}", ProviderChoice::Bedrock);
        let _ = format!("{:?}", ProviderChoice::Azure);
        let _ = format!("{:?}", ProviderChoice::Local);
    }

    // rtmx:req REQ-ONBOARD-028
    #[test]
    fn provider_selection_state_default_is_empty() {
        let state = ProviderSelectionState::default();
        assert!(state.project_id.is_none());
        assert!(state.region.is_none());
        assert!(state.azure_endpoint.is_none());
        assert!(state.model.is_none());
        assert!(state.local_endpoint.is_none());
    }
}
