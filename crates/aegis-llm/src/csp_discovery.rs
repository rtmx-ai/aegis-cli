//! CSP project discovery module.
//!
//! Discovers cloud service provider projects/accounts/subscriptions by
//! shelling out to the respective CLI tools (`gcloud`, `aws`, `az`).
//! Exposes a `CspDiscoverer` trait so tests can inject mocks.

use std::fmt;
use std::process::{Command, Stdio};
use std::time::Duration;

use thiserror::Error;

/// A project/account/subscription discovered from a CSP CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CspProject {
    pub id: String,
    pub name: String,
}

/// Errors from CSP project discovery.
#[derive(Debug, Error)]
pub enum CspDiscoveryError {
    #[error("{cli_name} not found on PATH")]
    SdkNotInstalled { cli_name: String },
    #[error("not authenticated: {guidance}")]
    NotAuthenticated { guidance: String },
    #[error("discovery timed out after 5 seconds")]
    Timeout,
    #[error("failed to parse CLI output: {detail}")]
    ParseError { detail: String },
}

impl CspDiscoveryError {
    /// Convert error to (message, guidance) tuple for TUI display.
    pub fn to_guidance(&self) -> (String, String) {
        match self {
            Self::SdkNotInstalled { cli_name } => (
                format!("{cli_name} not found on PATH"),
                format!("Install the {cli_name} CLI to enable project discovery"),
            ),
            Self::NotAuthenticated { guidance } => {
                ("Not authenticated".to_string(), guidance.clone())
            }
            Self::Timeout => (
                "Discovery timed out".to_string(),
                "CSP CLI took too long to respond. Check network connectivity.".to_string(),
            ),
            Self::ParseError { detail } => {
                ("Failed to parse CLI output".to_string(), detail.clone())
            }
        }
    }
}

impl fmt::Display for CspProject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.id)
    }
}

/// Trait for CSP project discovery, enabling mock injection in tests.
pub trait CspDiscoverer: Send + Sync {
    fn discover_projects(&self, provider: &str) -> Result<Vec<CspProject>, CspDiscoveryError>;
}

/// Real implementation that shells out to CSP CLIs.
pub struct CliCspDiscoverer;

impl CspDiscoverer for CliCspDiscoverer {
    fn discover_projects(&self, provider: &str) -> Result<Vec<CspProject>, CspDiscoveryError> {
        match provider {
            "vertex" => discover_gcp_projects(),
            "bedrock" => discover_aws_accounts(),
            "azure" => discover_azure_subscriptions(),
            "local" => Ok(vec![]),
            _ => Ok(vec![]),
        }
    }
}

// ---------------------------------------------------------------------------
// Timeout helper
// ---------------------------------------------------------------------------

/// Default timeout for CSP CLI commands.
const TIMEOUT_SECS: u64 = 5;

/// Run a command with a timeout. Returns the output on success, or
/// `CspDiscoveryError::Timeout` if the child does not finish in time.
fn run_with_timeout(
    cmd: &mut Command,
    timeout_secs: u64,
) -> Result<std::process::Output, CspDiscoveryError> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| CspDiscoveryError::Timeout)?;

    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child
                    .wait_with_output()
                    .map_err(|_| CspDiscoveryError::Timeout);
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CspDiscoveryError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Err(CspDiscoveryError::Timeout),
        }
    }
}

/// Check if a CLI tool is available on PATH.
fn cli_on_path(cli_name: &str) -> bool {
    Command::new("which")
        .arg(cli_name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// GCP
// ---------------------------------------------------------------------------

fn discover_gcp_projects() -> Result<Vec<CspProject>, CspDiscoveryError> {
    if !cli_on_path("gcloud") {
        return Err(CspDiscoveryError::SdkNotInstalled {
            cli_name: "gcloud".to_string(),
        });
    }

    // Auth check
    let auth_output = run_with_timeout(
        Command::new("gcloud").args(["auth", "print-access-token"]),
        TIMEOUT_SECS,
    )?;
    if !auth_output.status.success() {
        return Err(CspDiscoveryError::NotAuthenticated {
            guidance: "Run: gcloud auth login".to_string(),
        });
    }

    // List projects
    let output = run_with_timeout(
        Command::new("gcloud").args(["projects", "list", "--format=json"]),
        TIMEOUT_SECS,
    )?;
    if !output.status.success() {
        return Err(CspDiscoveryError::ParseError {
            detail: "gcloud projects list failed".to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_gcp_projects(&stdout)
}

/// Parse `gcloud projects list --format=json` output.
pub(crate) fn parse_gcp_projects(json: &str) -> Result<Vec<CspProject>, CspDiscoveryError> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| CspDiscoveryError::ParseError {
            detail: format!("invalid JSON: {e}"),
        })?;

    Ok(arr
        .iter()
        .filter_map(|entry| {
            let id = entry.get("projectId")?.as_str()?.to_string();
            let name = entry.get("name")?.as_str()?.to_string();
            Some(CspProject { id, name })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// AWS
// ---------------------------------------------------------------------------

fn discover_aws_accounts() -> Result<Vec<CspProject>, CspDiscoveryError> {
    if !cli_on_path("aws") {
        return Err(CspDiscoveryError::SdkNotInstalled {
            cli_name: "aws".to_string(),
        });
    }

    // Auth check via caller identity
    let id_output = run_with_timeout(
        Command::new("aws").args(["sts", "get-caller-identity", "--output", "json"]),
        TIMEOUT_SECS,
    )?;
    if !id_output.status.success() {
        return Err(CspDiscoveryError::NotAuthenticated {
            guidance: "Run: aws configure".to_string(),
        });
    }

    let id_stdout = String::from_utf8_lossy(&id_output.stdout);
    let fallback = parse_aws_caller_identity(&id_stdout)?;

    // Try organizations list-accounts
    let org_output = run_with_timeout(
        Command::new("aws").args(["organizations", "list-accounts", "--output", "json"]),
        TIMEOUT_SECS,
    );

    if let Ok(output) = org_output
        && output.status.success()
    {
        let org_stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(accounts) = parse_aws_org_accounts(&org_stdout)
            && !accounts.is_empty()
        {
            return Ok(accounts);
        }
    }

    // Fall back to single account from caller identity
    Ok(vec![CspProject {
        id: fallback.id,
        name: "Current Account".to_string(),
    }])
}

/// Parse `aws sts get-caller-identity` output, extracting the Account field.
pub(crate) fn parse_aws_caller_identity(json: &str) -> Result<CspProject, CspDiscoveryError> {
    let obj: serde_json::Value =
        serde_json::from_str(json).map_err(|e| CspDiscoveryError::ParseError {
            detail: format!("invalid JSON: {e}"),
        })?;

    let account = obj.get("Account").and_then(|v| v.as_str()).ok_or_else(|| {
        CspDiscoveryError::ParseError {
            detail: "missing Account field in caller identity".to_string(),
        }
    })?;

    Ok(CspProject {
        id: account.to_string(),
        name: "Current Account".to_string(),
    })
}

/// Parse `aws organizations list-accounts` output.
pub(crate) fn parse_aws_org_accounts(json: &str) -> Result<Vec<CspProject>, CspDiscoveryError> {
    let obj: serde_json::Value =
        serde_json::from_str(json).map_err(|e| CspDiscoveryError::ParseError {
            detail: format!("invalid JSON: {e}"),
        })?;

    let accounts = obj
        .get("Accounts")
        .and_then(|v| v.as_array())
        .ok_or_else(|| CspDiscoveryError::ParseError {
            detail: "missing Accounts array".to_string(),
        })?;

    Ok(accounts
        .iter()
        .filter_map(|entry| {
            let id = entry.get("Id")?.as_str()?.to_string();
            let name = entry.get("Name")?.as_str()?.to_string();
            Some(CspProject { id, name })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Azure
// ---------------------------------------------------------------------------

fn discover_azure_subscriptions() -> Result<Vec<CspProject>, CspDiscoveryError> {
    if !cli_on_path("az") {
        return Err(CspDiscoveryError::SdkNotInstalled {
            cli_name: "az".to_string(),
        });
    }

    let output = run_with_timeout(
        Command::new("az").args(["account", "list", "--output", "json"]),
        TIMEOUT_SECS,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("az login") || stderr.contains("not logged in") {
            return Err(CspDiscoveryError::NotAuthenticated {
                guidance: "Run: az login".to_string(),
            });
        }
        return Err(CspDiscoveryError::NotAuthenticated {
            guidance: "Run: az login".to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_azure_subscriptions(&stdout)
}

/// Parse `az account list --output json` output.
pub(crate) fn parse_azure_subscriptions(
    json: &str,
) -> Result<Vec<CspProject>, CspDiscoveryError> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| CspDiscoveryError::ParseError {
            detail: format!("invalid JSON: {e}"),
        })?;

    Ok(arr
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let name = entry.get("name")?.as_str()?.to_string();
            Some(CspProject { id, name })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockDiscoverer {
        projects: Mutex<Option<Vec<CspProject>>>,
        error_msg: Option<String>,
    }

    impl MockDiscoverer {
        fn with_projects(projects: Vec<CspProject>) -> Self {
            Self {
                projects: Mutex::new(Some(projects)),
                error_msg: None,
            }
        }

        fn with_error(cli_name: &str) -> Self {
            Self {
                projects: Mutex::new(None),
                error_msg: Some(cli_name.to_string()),
            }
        }
    }

    impl CspDiscoverer for MockDiscoverer {
        fn discover_projects(
            &self,
            _provider: &str,
        ) -> Result<Vec<CspProject>, CspDiscoveryError> {
            if let Some(cli_name) = &self.error_msg {
                return Err(CspDiscoveryError::SdkNotInstalled {
                    cli_name: cli_name.clone(),
                });
            }
            Ok(self.projects.lock().unwrap().take().unwrap_or_default())
        }
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_gcp_project_list_parses_json() {
        let json =
            r#"[{"projectId": "my-proj-123", "name": "My Project", "lifecycleState": "ACTIVE"}]"#;
        let projects = parse_gcp_projects(json).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "my-proj-123");
        assert_eq!(projects[0].name, "My Project");
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_gcp_project_list_empty_array() {
        let json = "[]";
        let projects = parse_gcp_projects(json).unwrap();
        assert!(projects.is_empty());
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_gcp_project_list_malformed_json() {
        let json = "not json at all";
        let result = parse_gcp_projects(json);
        assert!(matches!(result, Err(CspDiscoveryError::ParseError { .. })));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_aws_caller_identity_parses_account() {
        let json = r#"{"UserId": "AIDAEXAMPLE", "Account": "123456789012", "Arn": "arn:aws:iam::123456789012:user/test"}"#;
        let project = parse_aws_caller_identity(json).unwrap();
        assert_eq!(project.id, "123456789012");
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_aws_org_accounts_parses_json() {
        let json = r#"{"Accounts": [{"Id": "111111111111", "Name": "Production"}, {"Id": "222222222222", "Name": "Staging"}]}"#;
        let accounts = parse_aws_org_accounts(json).unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].id, "111111111111");
        assert_eq!(accounts[1].name, "Staging");
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_azure_subscription_list_parses_json() {
        let json = r#"[{"id": "sub-abc-123", "name": "Pay-As-You-Go", "state": "Enabled"}]"#;
        let subs = parse_azure_subscriptions(json).unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].id, "sub-abc-123");
        assert_eq!(subs[0].name, "Pay-As-You-Go");
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_local_provider_returns_empty() {
        let discoverer = CliCspDiscoverer;
        let result = discoverer.discover_projects("local");
        assert!(result.unwrap().is_empty());
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_unknown_provider_returns_empty() {
        let discoverer = CliCspDiscoverer;
        let result = discoverer.discover_projects("unknown");
        assert!(result.unwrap().is_empty());
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_error_to_guidance_sdk_not_installed() {
        let err = CspDiscoveryError::SdkNotInstalled {
            cli_name: "gcloud".into(),
        };
        let (msg, guide) = err.to_guidance();
        assert!(msg.contains("gcloud"));
        assert!(guide.contains("Install"));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_error_to_guidance_not_authenticated() {
        let err = CspDiscoveryError::NotAuthenticated {
            guidance: "Run: gcloud auth login".into(),
        };
        let (msg, guide) = err.to_guidance();
        assert!(msg.contains("authenticated"));
        assert!(guide.contains("gcloud auth login"));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_error_to_guidance_timeout() {
        let err = CspDiscoveryError::Timeout;
        let (msg, guide) = err.to_guidance();
        assert!(msg.contains("timed out"));
        assert!(guide.contains("network"));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_error_to_guidance_parse_error() {
        let err = CspDiscoveryError::ParseError {
            detail: "bad json".into(),
        };
        let (msg, guide) = err.to_guidance();
        assert!(msg.contains("parse"));
        assert!(guide.contains("bad json"));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_mock_discoverer_returns_projects() {
        let mock = MockDiscoverer::with_projects(vec![
            CspProject {
                id: "proj-1".into(),
                name: "Project One".into(),
            },
            CspProject {
                id: "proj-2".into(),
                name: "Project Two".into(),
            },
        ]);
        let projects = mock.discover_projects("vertex").unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].id, "proj-1");
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_mock_discoverer_returns_error() {
        let mock = MockDiscoverer::with_error("gcloud");
        let result = mock.discover_projects("vertex");
        assert!(matches!(
            result,
            Err(CspDiscoveryError::SdkNotInstalled { .. })
        ));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_gcp_project_list_multiple_projects() {
        let json = r#"[
            {"projectId": "proj-a", "name": "Alpha"},
            {"projectId": "proj-b", "name": "Beta"},
            {"projectId": "proj-c", "name": "Gamma"}
        ]"#;
        let projects = parse_gcp_projects(json).unwrap();
        assert_eq!(projects.len(), 3);
        assert_eq!(projects[2].name, "Gamma");
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_aws_caller_identity_missing_account_field() {
        let json = r#"{"UserId": "AIDA", "Arn": "arn:aws:iam::123:user/x"}"#;
        let result = parse_aws_caller_identity(json);
        assert!(matches!(result, Err(CspDiscoveryError::ParseError { .. })));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_aws_org_accounts_missing_accounts_key() {
        let json = r#"{"Something": []}"#;
        let result = parse_aws_org_accounts(json);
        assert!(matches!(result, Err(CspDiscoveryError::ParseError { .. })));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_azure_subscription_list_empty() {
        let json = "[]";
        let subs = parse_azure_subscriptions(json).unwrap();
        assert!(subs.is_empty());
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_csp_project_display() {
        let p = CspProject {
            id: "proj-1".into(),
            name: "My Project".into(),
        };
        assert_eq!(format!("{p}"), "My Project (proj-1)");
    }
}
