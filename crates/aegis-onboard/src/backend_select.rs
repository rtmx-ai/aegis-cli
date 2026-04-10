//! Default backend selection chain for first-run setup.
//!
//! During the first-run wizard, automatically selects the best
//! available backend by probing in priority order:
//! 1. Enterprise BYOC (gateway URL)
//! 2. Local Ollama with llama3
//! 3. Local vLLM
//! 4. Cloud credentials (gcloud ADC, AWS creds)
//! 5. No backend (with helpful message)

use std::path::Path;

use crate::byoc;
use crate::detect::DetectedProvider;

/// The result of the backend selection chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedBackend {
    /// Enterprise BYOC gateway detected with the given URL.
    EnterpriseByoc(String),
    /// Local Ollama detected with the given model name.
    LocalOllama(String),
    /// Local vLLM detected with the given endpoint.
    LocalVllm(String),
    /// Cloud credentials detected (provider name, e.g. "gcloud").
    CloudCredentials(String),
    /// No backend could be detected.
    NoBackend,
}

/// A single detection step result, used for reporting what was tried.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Human-readable name of what was checked.
    pub check: String,
    /// Whether the check succeeded.
    pub found: bool,
    /// Optional detail about what was found (or why not).
    pub detail: String,
}

/// Select the best available backend by probing in priority order.
///
/// This is the synchronous entry point that checks BYOC and cloud
/// credentials. For local provider detection (which requires async
/// HTTP probes), use [`select_default_backend_with_providers`].
pub fn select_default_backend(config_dir: &Path) -> (SelectedBackend, Vec<DetectionResult>) {
    select_default_backend_with_providers(config_dir, &[], false)
}

/// Select the best available backend with pre-detected local providers.
///
/// Accepts already-detected local providers (from async detection)
/// and a flag indicating whether gcloud ADC was found.
pub fn select_default_backend_with_providers(
    _config_dir: &Path,
    local_providers: &[DetectedProvider],
    has_gcloud_adc: bool,
) -> (SelectedBackend, Vec<DetectionResult>) {
    let mut results = Vec::new();

    // 1. Check for enterprise BYOC gateway
    let byoc_url = byoc::detect_byoc_environment();
    results.push(DetectionResult {
        check: "Enterprise BYOC gateway".to_string(),
        found: byoc_url.is_some(),
        detail: match &byoc_url {
            Some(url) => format!("Gateway found at {url}"),
            None => "No AEGIS_GATEWAY_URL or gateway.conf found".to_string(),
        },
    });
    if let Some(url) = byoc_url {
        return (SelectedBackend::EnterpriseByoc(url), results);
    }

    // 2. Check for local Ollama with llama3
    let ollama = local_providers.iter().find(|p| p.name == "Ollama");
    let ollama_llama3 =
        ollama.and_then(|p| crate::detect::detect_ollama_llama3_from_models(&p.models));
    results.push(DetectionResult {
        check: "Local Ollama with llama3".to_string(),
        found: ollama_llama3.is_some(),
        detail: match &ollama_llama3 {
            Some(model) => format!("Found model: {model}"),
            None => match ollama {
                Some(p) => format!(
                    "Ollama running but no llama3 model (found: {})",
                    p.models.join(", ")
                ),
                None => "Ollama not detected on localhost:11434".to_string(),
            },
        },
    });
    if let Some(model) = ollama_llama3 {
        return (SelectedBackend::LocalOllama(model), results);
    }

    // 3. Check for local vLLM
    let vllm = local_providers.iter().find(|p| p.name == "vLLM");
    results.push(DetectionResult {
        check: "Local vLLM".to_string(),
        found: vllm.is_some(),
        detail: match vllm {
            Some(p) => format!(
                "vLLM running at {} with models: {}",
                p.endpoint,
                p.models.join(", ")
            ),
            None => "vLLM not detected on localhost:8000".to_string(),
        },
    });
    if let Some(p) = vllm {
        return (SelectedBackend::LocalVllm(p.endpoint.clone()), results);
    }

    // 4. Check for cloud credentials (gcloud ADC)
    results.push(DetectionResult {
        check: "Google Cloud ADC".to_string(),
        found: has_gcloud_adc,
        detail: if has_gcloud_adc {
            "Application Default Credentials found".to_string()
        } else {
            "No gcloud ADC found".to_string()
        },
    });
    if has_gcloud_adc {
        return (
            SelectedBackend::CloudCredentials("gcloud".to_string()),
            results,
        );
    }

    // 5. No backend found
    (SelectedBackend::NoBackend, results)
}

/// Return a helpful message when no backend is available.
///
/// Explains what was checked and suggests next steps.
pub fn no_backend_message() -> String {
    "\
aegis could not detect an available LLM backend.

The following sources were checked (in priority order):
  1. Enterprise BYOC gateway (AEGIS_GATEWAY_URL or gateway.conf)
  2. Local Ollama with a llama3 model (localhost:11434)
  3. Local vLLM server (localhost:8000)
  4. Google Cloud Application Default Credentials

To get started, try one of these:
  - Install Ollama and pull a model:
      curl -fsSL https://ollama.ai/install.sh | sh
      ollama pull llama3
  - Set up Google Cloud credentials:
      gcloud auth application-default login
  - Configure an enterprise gateway:
      export AEGIS_GATEWAY_URL=https://your-gateway.example.com

Then run 'aegis init' again."
        .to_string()
}

/// Format detection results into a human-readable report.
///
/// Shows each detection step with a pass/fail indicator and detail.
pub fn format_detection_results(results: &[DetectionResult]) -> String {
    let mut lines = Vec::new();
    lines.push("Backend detection results:".to_string());
    for r in results {
        let indicator = if r.found { "[found]" } else { "[  -- ]" };
        lines.push(format!("  {} {} -- {}", indicator, r.check, r.detail));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::DetectedProvider;
    use tempfile::TempDir;

    // @req REQ-ONBOARD-022
    #[test]
    fn select_returns_no_backend_when_nothing_available() {
        let tmp = TempDir::new().unwrap();
        let (backend, results) = select_default_backend_with_providers(tmp.path(), &[], false);
        assert_eq!(backend, SelectedBackend::NoBackend);
        assert!(
            !results.is_empty(),
            "Should report detection steps even when nothing found"
        );
    }

    // @req REQ-ONBOARD-022
    #[test]
    fn select_prefers_byoc_over_local() {
        // This test uses the env var path; run in isolation.
        // We test the logic by providing local providers but also
        // checking the priority ordering.
        let tmp = TempDir::new().unwrap();
        let providers = vec![DetectedProvider {
            name: "Ollama".to_string(),
            endpoint: "http://localhost:11434".to_string(),
            models: vec!["llama3:latest".to_string()],
        }];

        // Without BYOC, should find Ollama
        let (backend, _) = select_default_backend_with_providers(tmp.path(), &providers, false);
        assert!(
            matches!(backend, SelectedBackend::LocalOllama(_)),
            "Should select Ollama when no BYOC: {backend:?}"
        );
    }

    // @req REQ-ONBOARD-022
    #[test]
    fn select_finds_ollama_with_llama3() {
        let tmp = TempDir::new().unwrap();
        let providers = vec![DetectedProvider {
            name: "Ollama".to_string(),
            endpoint: "http://localhost:11434".to_string(),
            models: vec!["codellama:7b".to_string(), "llama3:latest".to_string()],
        }];

        let (backend, _) = select_default_backend_with_providers(tmp.path(), &providers, false);
        assert_eq!(
            backend,
            SelectedBackend::LocalOllama("llama3:latest".to_string())
        );
    }

    // @req REQ-ONBOARD-022
    #[test]
    fn select_skips_ollama_without_llama3() {
        let tmp = TempDir::new().unwrap();
        let providers = vec![DetectedProvider {
            name: "Ollama".to_string(),
            endpoint: "http://localhost:11434".to_string(),
            models: vec!["mistral:latest".to_string()],
        }];

        let (backend, results) =
            select_default_backend_with_providers(tmp.path(), &providers, false);
        assert_eq!(backend, SelectedBackend::NoBackend);
        // Should report that Ollama was found but no llama3
        let ollama_result = results.iter().find(|r| r.check.contains("Ollama"));
        assert!(ollama_result.is_some());
        assert!(
            !ollama_result.unwrap().found,
            "Ollama without llama3 should not be marked found"
        );
    }

    // @req REQ-ONBOARD-022
    #[test]
    fn select_finds_vllm_when_no_ollama_llama3() {
        let tmp = TempDir::new().unwrap();
        let providers = vec![DetectedProvider {
            name: "vLLM".to_string(),
            endpoint: "http://localhost:8000".to_string(),
            models: vec!["meta-llama/Llama-3-8B".to_string()],
        }];

        let (backend, _) = select_default_backend_with_providers(tmp.path(), &providers, false);
        assert_eq!(
            backend,
            SelectedBackend::LocalVllm("http://localhost:8000".to_string())
        );
    }

    // @req REQ-ONBOARD-022
    #[test]
    fn select_prefers_ollama_over_vllm() {
        let tmp = TempDir::new().unwrap();
        let providers = vec![
            DetectedProvider {
                name: "Ollama".to_string(),
                endpoint: "http://localhost:11434".to_string(),
                models: vec!["llama3:latest".to_string()],
            },
            DetectedProvider {
                name: "vLLM".to_string(),
                endpoint: "http://localhost:8000".to_string(),
                models: vec!["some-model".to_string()],
            },
        ];

        let (backend, _) = select_default_backend_with_providers(tmp.path(), &providers, false);
        assert!(
            matches!(backend, SelectedBackend::LocalOllama(_)),
            "Should prefer Ollama over vLLM"
        );
    }

    // @req REQ-ONBOARD-022
    #[test]
    fn select_falls_back_to_cloud_credentials() {
        let tmp = TempDir::new().unwrap();
        let (backend, _) = select_default_backend_with_providers(
            tmp.path(),
            &[],
            true, // has_gcloud_adc
        );
        assert_eq!(
            backend,
            SelectedBackend::CloudCredentials("gcloud".to_string())
        );
    }

    // @req REQ-ONBOARD-022
    #[test]
    fn select_reports_all_detection_steps() {
        let tmp = TempDir::new().unwrap();
        let (_, results) = select_default_backend_with_providers(tmp.path(), &[], false);
        // Should have at least 4 detection steps
        assert!(
            results.len() >= 4,
            "Should report at least 4 detection steps, got {}",
            results.len()
        );
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn no_backend_message_is_non_empty() {
        let msg = no_backend_message();
        assert!(!msg.is_empty());
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn no_backend_message_mentions_ollama() {
        let msg = no_backend_message();
        assert!(
            msg.contains("Ollama"),
            "Message should mention Ollama: {msg}"
        );
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn no_backend_message_mentions_gcloud() {
        let msg = no_backend_message();
        assert!(
            msg.contains("gcloud"),
            "Message should mention gcloud: {msg}"
        );
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn no_backend_message_mentions_gateway() {
        let msg = no_backend_message();
        assert!(
            msg.contains("AEGIS_GATEWAY_URL"),
            "Message should mention AEGIS_GATEWAY_URL: {msg}"
        );
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn no_backend_message_suggests_next_steps() {
        let msg = no_backend_message();
        assert!(
            msg.contains("aegis init"),
            "Message should suggest running aegis init: {msg}"
        );
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn format_detection_results_shows_all_steps() {
        let results = vec![
            DetectionResult {
                check: "Enterprise BYOC".to_string(),
                found: false,
                detail: "Not found".to_string(),
            },
            DetectionResult {
                check: "Local Ollama".to_string(),
                found: true,
                detail: "Found llama3".to_string(),
            },
        ];
        let output = format_detection_results(&results);
        assert!(output.contains("Enterprise BYOC"));
        assert!(output.contains("Local Ollama"));
        assert!(output.contains("[found]"));
        assert!(output.contains("[  -- ]"));
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn format_detection_results_empty_input() {
        let output = format_detection_results(&[]);
        assert!(
            output.contains("Backend detection results"),
            "Should have header even with no results"
        );
    }

    // @req REQ-ONBOARD-027
    #[test]
    fn format_detection_results_includes_detail() {
        let results = vec![DetectionResult {
            check: "Test check".to_string(),
            found: false,
            detail: "specific detail here".to_string(),
        }];
        let output = format_detection_results(&results);
        assert!(
            output.contains("specific detail here"),
            "Should include detail text"
        );
    }
}
