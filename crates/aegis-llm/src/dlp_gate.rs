//! DLP endpoint classification and transmission gate (REQ-SECURITY-018).
//!
//! Classifies outbound LLM endpoints as `Government`, `Commercial`,
//! `Local`, or `Unknown` and decides whether a piece of content may be
//! transmitted given its DLP classification.
//!
//! Policy summary:
//!
//! | Endpoint      | CUI markings      | PII               |
//! |---------------|-------------------|-------------------|
//! | Local         | Allow             | Allow             |
//! | Government    | Allow (logged)    | Allow (logged)    |
//! | Commercial    | **Block**         | **Block**         |
//! | Unknown       | **Block**         | Allow (warn)      |
//!
//! This is a library-level primitive. Wiring the gate into the provider
//! stream path is a separate integration (future work); this module only
//! exposes [`DlpTransmissionGate::check`].

use aegis_security::dlp::{DlpCategory, DlpMatch, DlpScanner};

/// Classification of the remote endpoint, controlling what content may
/// be transmitted to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointClassification {
    /// Government / IL4 / IL5 endpoint. May receive CUI and PII.
    Government,
    /// Commercial endpoint. CUI and PII must be blocked.
    Commercial,
    /// Local loopback endpoint (localhost / 127.0.0.1 / ::1).
    /// Anything is allowed; no egress off the machine.
    Local,
    /// Classification could not be determined. Default to the most
    /// restrictive rule for CUI; PII is allowed with a warning.
    Unknown,
}

/// Classify a remote endpoint based on its URL.
///
/// The classifier is intentionally conservative: if the host does not
/// match a known-government or known-local pattern, it is either
/// classified as `Commercial` (for known commercial LLM hosts) or
/// `Unknown` (default).
///
/// # Heuristics
///
/// * Loopback hosts (`localhost`, `127.0.0.1`, `::1`) are `Local`.
/// * GCP Vertex AI Assured Workloads is identified by a regional
///   `*-aiplatform.googleapis.com` host where the region is one of the
///   US public-sector regions (see [`GOV_GCP_REGIONS`]) and the path
///   references `/aiplatform/` or `/publishers/` or `/projects/`.
/// * AWS GovCloud is identified by `*.amazonaws.com` with a
///   `us-gov-*` region segment in the host.
/// * Azure Government is `*.azure.us` or `*.usgovcloudapi.net`.
/// * Known commercial LLM hosts (`api.openai.com`, `api.anthropic.com`,
///   `generativelanguage.googleapis.com`) are `Commercial`.
/// * Everything else is `Unknown`.
pub fn classify_endpoint(endpoint_url: &str) -> EndpointClassification {
    // Best-effort host extraction without a URL parser dependency.
    // Accept inputs like "http://host:port/path", "https://host/path",
    // bare "host:port/path", or plain "host".
    let trimmed = endpoint_url.trim();
    let without_scheme = match trimmed.split_once("://") {
        Some((_scheme, rest)) => rest,
        None => trimmed,
    };

    // Strip authority @ section if present (user:pass@host).
    let after_at = match without_scheme.rsplit_once('@') {
        Some((_, host_and_rest)) => host_and_rest,
        None => without_scheme,
    };

    // Separate host (possibly bracketed IPv6) from the rest.
    let (host_raw, path) = if let Some(rest) = after_at.strip_prefix('[') {
        // IPv6 literal: find closing bracket.
        match rest.find(']') {
            Some(end) => {
                let host = &rest[..end];
                let remainder = &rest[end + 1..];
                // remainder may start with :port and then /path.
                let path = remainder.find('/').map(|i| &remainder[i..]).unwrap_or("");
                (host, path)
            }
            None => (rest, ""),
        }
    } else {
        let (host_port, path) = match after_at.find('/') {
            Some(idx) => (&after_at[..idx], &after_at[idx..]),
            None => (after_at, ""),
        };
        // Strip :port from non-IPv6 host.
        let host = match host_port.rfind(':') {
            Some(idx) => &host_port[..idx],
            None => host_port,
        };
        (host, path)
    };

    let host = host_raw.to_ascii_lowercase();
    let path_lower = path.to_ascii_lowercase();

    // -- Local / loopback --
    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".localhost")
    {
        return EndpointClassification::Local;
    }

    // -- Azure Government --
    if host.ends_with(".azure.us") || host.ends_with(".usgovcloudapi.net") {
        return EndpointClassification::Government;
    }

    // -- AWS GovCloud: <service>.us-gov-<region>.amazonaws.com --
    if host.ends_with(".amazonaws.com") && host.contains(".us-gov-") {
        return EndpointClassification::Government;
    }

    // -- Known commercial LLM hosts (checked before the generic
    //    googleapis check so they aren't mis-classified) --
    const COMMERCIAL_HOSTS: &[&str] = &[
        "api.openai.com",
        "api.anthropic.com",
        "generativelanguage.googleapis.com",
    ];
    if COMMERCIAL_HOSTS.iter().any(|h| &host == h) {
        return EndpointClassification::Commercial;
    }

    // -- GCP Vertex AI Assured Workloads --
    //
    // Regional hostname e.g. "us-central1-aiplatform.googleapis.com".
    // Vertex AI only operates in the documented Assured Workloads
    // regions; a path that references the aiplatform API is a strong
    // signal. Without the path we fall back to Commercial for
    // non-assured regions so an unintentional `generativelanguage`
    // call does not slip through as Government.
    if host.ends_with(".googleapis.com") && host.contains("-aiplatform") {
        let region = host
            .strip_suffix("-aiplatform.googleapis.com")
            .unwrap_or_default();
        let looks_like_aiplatform = path_lower.contains("/aiplatform/")
            || path_lower.contains("/publishers/")
            || path_lower.contains("/projects/");
        if looks_like_aiplatform && GOV_GCP_REGIONS.contains(&region) {
            return EndpointClassification::Government;
        }
        // Regional aiplatform host but without the Assured Workloads
        // region or path markers: treat as commercial to be safe.
        return EndpointClassification::Commercial;
    }

    EndpointClassification::Unknown
}

/// GCP Assured Workloads regions that may receive CUI workloads.
/// Kept intentionally small; extend as Google publishes additional
/// IL4/IL5-eligible regions.
const GOV_GCP_REGIONS: &[&str] = &[
    "us-central1",
    "us-east1",
    "us-east4",
    "us-east5",
    "us-west1",
    "us-west2",
    "us-west3",
    "us-west4",
    "us-south1",
];

/// Decision returned by [`DlpTransmissionGate::check`].
#[derive(Debug)]
pub enum DlpGateDecision {
    /// The content may be transmitted.
    Allow,
    /// The content must not be transmitted. `reason` is a concise
    /// human-readable explanation safe for display; `matches` carries
    /// the full scanner findings for audit / debugging.
    Block {
        reason: String,
        matches: Vec<DlpMatch>,
    },
}

/// Gate that inspects outbound content against the DLP scanner and
/// enforces the endpoint-classification policy.
pub struct DlpTransmissionGate {
    scanner: DlpScanner,
}

impl Default for DlpTransmissionGate {
    fn default() -> Self {
        Self::new()
    }
}

impl DlpTransmissionGate {
    /// Create a gate backed by the default DLP scanner.
    pub fn new() -> Self {
        Self {
            scanner: DlpScanner::new(),
        }
    }

    /// Decide whether `content` may be transmitted to `endpoint_url`.
    ///
    /// See the module-level documentation for the policy table.
    pub fn check(&self, content: &str, endpoint_url: &str) -> DlpGateDecision {
        let classification = classify_endpoint(endpoint_url);

        // Local endpoints are unconditionally allowed; skip the scan
        // entirely to keep the hot path cheap for air-gapped users.
        if classification == EndpointClassification::Local {
            return DlpGateDecision::Allow;
        }

        let matches = self.scanner.scan(content);
        let has_cui = matches
            .iter()
            .any(|m| m.category == DlpCategory::CuiMarking);
        let has_pii = matches.iter().any(|m| is_pii(m.category));

        match classification {
            EndpointClassification::Local => DlpGateDecision::Allow,

            EndpointClassification::Government => {
                if has_cui || has_pii {
                    tracing::info!(
                        cui = has_cui,
                        pii = has_pii,
                        endpoint = endpoint_url,
                        matches = matches.len(),
                        "DLP: sensitive content permitted to government endpoint"
                    );
                }
                DlpGateDecision::Allow
            }

            EndpointClassification::Commercial => {
                if has_cui {
                    tracing::warn!(
                        endpoint = endpoint_url,
                        "DLP: CUI transmission to commercial endpoint blocked"
                    );
                    return DlpGateDecision::Block {
                        reason: "CUI markings detected; transmission to commercial \
                                 endpoint is prohibited"
                            .to_string(),
                        matches,
                    };
                }
                if has_pii {
                    tracing::warn!(
                        endpoint = endpoint_url,
                        "DLP: PII transmission to commercial endpoint blocked"
                    );
                    return DlpGateDecision::Block {
                        reason: "PII detected; transmission to commercial endpoint is \
                                 prohibited"
                            .to_string(),
                        matches,
                    };
                }
                DlpGateDecision::Allow
            }

            EndpointClassification::Unknown => {
                if has_cui {
                    tracing::warn!(
                        endpoint = endpoint_url,
                        "DLP: CUI transmission to unknown endpoint blocked"
                    );
                    return DlpGateDecision::Block {
                        reason: "CUI markings detected; endpoint classification is \
                                 Unknown, defaulting to block"
                            .to_string(),
                        matches,
                    };
                }
                if has_pii {
                    tracing::warn!(
                        endpoint = endpoint_url,
                        matches = matches.len(),
                        "DLP: PII transmission to unknown endpoint allowed with warn"
                    );
                }
                DlpGateDecision::Allow
            }
        }
    }
}

/// Return true if a category represents PII subject to the gate.
/// `IpAddress` and `ApiKey` are informational in this context and do
/// not independently trigger a PII block (they flag at the scanner
/// level but are allowed to government endpoints; commercial blocking
/// by policy centres on CUI markings and direct-identity PII).
fn is_pii(category: DlpCategory) -> bool {
    matches!(
        category,
        DlpCategory::Ssn
            | DlpCategory::Email
            | DlpCategory::PhoneNumber
            | DlpCategory::CreditCard
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-SECURITY-018
    #[test]
    fn classify_localhost_variants() {
        for url in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:11434/",
            "http://service.localhost:8080/",
        ] {
            assert_eq!(
                classify_endpoint(url),
                EndpointClassification::Local,
                "{url} should classify as Local"
            );
        }
    }

    // rtmx:req REQ-SECURITY-018
    #[test]
    fn classify_government_endpoints() {
        let govs = [
            "https://bedrock.us-gov-west-1.amazonaws.com/",
            "https://my.azure.us/",
            "https://example.usgovcloudapi.net/openai",
            "https://us-central1-aiplatform.googleapis.com/v1/projects/p/locations/us-central1/publishers/google/models/x:predict",
        ];
        for url in govs {
            assert_eq!(
                classify_endpoint(url),
                EndpointClassification::Government,
                "{url} should classify as Government"
            );
        }
    }

    // rtmx:req REQ-SECURITY-018
    #[test]
    fn classify_commercial_endpoints() {
        for url in [
            "https://api.openai.com/v1/chat/completions",
            "https://api.anthropic.com/v1/messages",
            "https://generativelanguage.googleapis.com/v1beta/models/x:streamGenerateContent",
        ] {
            assert_eq!(
                classify_endpoint(url),
                EndpointClassification::Commercial,
                "{url} should classify as Commercial"
            );
        }
    }

    // rtmx:req REQ-SECURITY-018
    #[test]
    fn classify_unknown_fallback() {
        assert_eq!(
            classify_endpoint("https://example.org/api"),
            EndpointClassification::Unknown
        );
        assert_eq!(
            classify_endpoint("https://llm.internal.corp/v1"),
            EndpointClassification::Unknown
        );
    }

    // rtmx:req REQ-SECURITY-018
    #[test]
    fn gate_allows_clean_content_to_commercial() {
        let gate = DlpTransmissionGate::new();
        let decision = gate.check("just a question", "https://api.openai.com/");
        assert!(matches!(decision, DlpGateDecision::Allow));
    }

    // rtmx:req REQ-SECURITY-018
    #[test]
    fn gate_blocks_cui_to_commercial() {
        let gate = DlpTransmissionGate::new();
        let decision = gate.check(
            "Marked CUI//SP-CTI release",
            "https://api.openai.com/v1/chat/completions",
        );
        assert!(matches!(decision, DlpGateDecision::Block { .. }));
    }
}
