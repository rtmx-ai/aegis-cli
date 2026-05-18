//! Model provenance and origin-based access control.
//!
//! Maps known model family prefixes to their country of origin, then
//! applies a configurable policy that assigns each country an access
//! tier (Approved / ReviewRequired / Denied). Default policy denies
//! PRC-origin and unclassified models, consistent with U.S. national
//! security guidance for CUI environments.

use std::collections::HashMap;
use std::fmt;

// ---- Country of origin ----

/// Country (or region) where a model family was developed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CountryOfOrigin {
    UnitedStates,
    France,
    UnitedArabEmirates,
    Germany,
    China,
    /// Model family not in the registry.
    Unknown,
}

impl fmt::Display for CountryOfOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnitedStates => write!(f, "US"),
            Self::France => write!(f, "France"),
            Self::UnitedArabEmirates => write!(f, "UAE"),
            Self::Germany => write!(f, "Germany"),
            Self::China => write!(f, "China"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

// ---- Origin tier (policy decision) ----

/// Access tier assigned to a model based on its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginTier {
    /// Model approved for use.
    Approved,
    /// Model usable but logged with a warning.
    ReviewRequired,
    /// Model blocked from use and download.
    Denied,
}

impl fmt::Display for OriginTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Approved => write!(f, "approved"),
            Self::ReviewRequired => write!(f, "review_required"),
            Self::Denied => write!(f, "denied"),
        }
    }
}

// ---- Static origin registry ----

/// Known model family prefix -> country of origin.
///
/// Checked in order; first matching prefix wins. Keep entries ordered
/// from most-specific to least-specific within each family (same
/// convention as `KNOWN_MODELS` in capabilities.rs).
const KNOWN_MODEL_ORIGINS: &[(&str, CountryOfOrigin)] = &[
    // United States
    ("llama3", CountryOfOrigin::UnitedStates),
    ("llama2", CountryOfOrigin::UnitedStates),
    ("codellama", CountryOfOrigin::UnitedStates),
    ("llama", CountryOfOrigin::UnitedStates),
    ("gemma4", CountryOfOrigin::UnitedStates),
    ("gemma3", CountryOfOrigin::UnitedStates),
    ("gemma2", CountryOfOrigin::UnitedStates),
    ("gemma", CountryOfOrigin::UnitedStates),
    ("phi-4", CountryOfOrigin::UnitedStates),
    ("phi-3", CountryOfOrigin::UnitedStates),
    ("phi", CountryOfOrigin::UnitedStates),
    ("starcoder", CountryOfOrigin::UnitedStates),
    ("granite", CountryOfOrigin::UnitedStates),
    ("gpt-oss", CountryOfOrigin::UnitedStates),
    ("nomic-embed", CountryOfOrigin::UnitedStates),
    ("all-minilm", CountryOfOrigin::UnitedStates),
    ("snowflake", CountryOfOrigin::UnitedStates),
    ("command-r", CountryOfOrigin::UnitedStates),
    // France
    ("mistral", CountryOfOrigin::France),
    ("mixtral", CountryOfOrigin::France),
    ("codestral", CountryOfOrigin::France),
    ("mathstral", CountryOfOrigin::France),
    ("pixtral", CountryOfOrigin::France),
    // China (People's Republic)
    ("qwen", CountryOfOrigin::China),
    ("deepseek", CountryOfOrigin::China),
    ("yi", CountryOfOrigin::China),
    ("baichuan", CountryOfOrigin::China),
    ("chatglm", CountryOfOrigin::China),
    ("internlm", CountryOfOrigin::China),
    ("glm", CountryOfOrigin::China),
    // UAE
    ("falcon", CountryOfOrigin::UnitedArabEmirates),
    // Germany (multinational, led from Germany)
    ("bloom", CountryOfOrigin::Germany),
];

/// Look up the country of origin for a model name.
///
/// Matches the model name (lowercased) against known prefixes.
/// Returns `CountryOfOrigin::Unknown` if no prefix matches.
pub fn lookup_origin(model: &str) -> CountryOfOrigin {
    let lower = model.to_lowercase();
    for (prefix, origin) in KNOWN_MODEL_ORIGINS {
        if lower.starts_with(prefix) {
            return *origin;
        }
    }
    CountryOfOrigin::Unknown
}

// ---- Model origin policy ----

/// Default country -> tier mapping. Reflects U.S. national security
/// posture: allied nations approved, adversary nations denied,
/// unknown models denied (default-deny).
fn default_tier(country: CountryOfOrigin) -> OriginTier {
    match country {
        CountryOfOrigin::UnitedStates => OriginTier::Approved,
        CountryOfOrigin::France => OriginTier::Approved,
        CountryOfOrigin::UnitedArabEmirates => OriginTier::Approved,
        CountryOfOrigin::Germany => OriginTier::Approved,
        CountryOfOrigin::China => OriginTier::Denied,
        CountryOfOrigin::Unknown => OriginTier::Denied,
    }
}

/// Configurable origin policy. Site administrators can override
/// the default tier for any country via config.yaml.
#[derive(Debug, Clone, Default)]
pub struct ModelOriginPolicy {
    overrides: HashMap<CountryOfOrigin, OriginTier>,
    allow_unclassified: bool,
}

impl ModelOriginPolicy {
    /// Create a policy with explicit overrides.
    pub fn with_overrides(overrides: HashMap<CountryOfOrigin, OriginTier>) -> Self {
        Self {
            overrides,
            allow_unclassified: false,
        }
    }

    /// Enable the `--allow-unclassified-models` escape hatch.
    /// Promotes Unknown from Denied to ReviewRequired.
    pub fn allow_unclassified(mut self) -> Self {
        self.allow_unclassified = true;
        self
    }

    /// Evaluate the tier for a given country under this policy.
    pub fn tier_for(&self, country: CountryOfOrigin) -> OriginTier {
        if let Some(&tier) = self.overrides.get(&country) {
            return tier;
        }
        if country == CountryOfOrigin::Unknown && self.allow_unclassified {
            return OriginTier::ReviewRequired;
        }
        default_tier(country)
    }

    /// Evaluate a model name: look up origin, then apply policy.
    /// Returns the origin, tier, and a human-readable reason.
    pub fn evaluate(&self, model: &str) -> ModelPolicyDecision {
        let origin = lookup_origin(model);
        let tier = self.tier_for(origin);
        let reason = match tier {
            OriginTier::Approved => format!("Model '{}' approved: {} origin", model, origin),
            OriginTier::ReviewRequired => {
                format!("Model '{}' allowed with review: {} origin", model, origin)
            }
            OriginTier::Denied => format!(
                "Model '{}' restricted: {} origin (denied by model origin policy)",
                model, origin
            ),
        };
        ModelPolicyDecision {
            model_name: model.to_string(),
            origin,
            tier,
            reason,
        }
    }
}

/// Result of evaluating a model against the origin policy.
#[derive(Debug, Clone)]
pub struct ModelPolicyDecision {
    pub model_name: String,
    pub origin: CountryOfOrigin,
    pub tier: OriginTier,
    pub reason: String,
}

impl ModelPolicyDecision {
    /// Whether the model is usable (Approved or ReviewRequired).
    pub fn is_allowed(&self) -> bool {
        !matches!(self.tier, OriginTier::Denied)
    }
}

// ---- Air-gapped model manifest (REQ-TUI-110) ----

/// A pre-approved model entry in an air-gapped manifest.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestEntry {
    pub name: String,
    pub sha256: String,
}

/// Air-gapped model manifest: restricts available models to pre-loaded set.
///
/// In air-gapped deployments, only models listed in `~/.aegis/model_manifest.toml`
/// are available. Each entry includes a SHA-256 hash for integrity verification.
/// The manifest is signed by an admin during side-load preparation.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ModelManifest {
    #[serde(default)]
    pub models: Vec<ManifestEntry>,
}

impl ModelManifest {
    /// Parse a manifest from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        toml::from_str(text).map_err(|e| format!("manifest parse error: {e}"))
    }

    /// Load manifest from a file path, returning None if the file doesn't exist.
    pub fn load(path: &std::path::Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        Self::from_toml(&text).ok()
    }

    /// Check if a model name is in the manifest.
    pub fn contains(&self, model_name: &str) -> bool {
        self.models.iter().any(|e| e.name == model_name)
    }

    /// List of model names in the manifest.
    pub fn model_names(&self) -> Vec<&str> {
        self.models.iter().map(|e| e.name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- REQ-LLM-043: Static model origin registry ---

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_llama_origin_is_us() {
        assert_eq!(
            lookup_origin("llama3:latest"),
            CountryOfOrigin::UnitedStates
        );
        assert_eq!(lookup_origin("llama3-8b"), CountryOfOrigin::UnitedStates);
        assert_eq!(lookup_origin("llama2"), CountryOfOrigin::UnitedStates);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_codellama_origin_is_us() {
        assert_eq!(
            lookup_origin("codellama:13b"),
            CountryOfOrigin::UnitedStates
        );
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_gemma_origin_is_us() {
        assert_eq!(
            lookup_origin("gemma4:latest"),
            CountryOfOrigin::UnitedStates
        );
        assert_eq!(lookup_origin("gemma3:2b"), CountryOfOrigin::UnitedStates);
        assert_eq!(lookup_origin("gemma2"), CountryOfOrigin::UnitedStates);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_phi_origin_is_us() {
        assert_eq!(lookup_origin("phi-4"), CountryOfOrigin::UnitedStates);
        assert_eq!(lookup_origin("phi-3:mini"), CountryOfOrigin::UnitedStates);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_granite_origin_is_us() {
        assert_eq!(
            lookup_origin("granite-3.3-2b"),
            CountryOfOrigin::UnitedStates
        );
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_gpt_oss_origin_is_us() {
        assert_eq!(lookup_origin("gpt-oss"), CountryOfOrigin::UnitedStates);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_mistral_origin_is_france() {
        assert_eq!(
            lookup_origin("mistral-7b-instruct"),
            CountryOfOrigin::France
        );
        assert_eq!(lookup_origin("mixtral-8x7b"), CountryOfOrigin::France);
        assert_eq!(lookup_origin("codestral"), CountryOfOrigin::France);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_qwen_origin_is_china() {
        assert_eq!(lookup_origin("qwen:7b"), CountryOfOrigin::China);
        assert_eq!(lookup_origin("qwen2.5-coder"), CountryOfOrigin::China);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_deepseek_origin_is_china() {
        assert_eq!(lookup_origin("deepseek-r1:8b"), CountryOfOrigin::China);
        assert_eq!(lookup_origin("deepseek-coder"), CountryOfOrigin::China);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_yi_origin_is_china() {
        assert_eq!(lookup_origin("yi:34b"), CountryOfOrigin::China);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_falcon_origin_is_uae() {
        assert_eq!(
            lookup_origin("falcon:40b"),
            CountryOfOrigin::UnitedArabEmirates
        );
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_bloom_origin_is_germany() {
        assert_eq!(lookup_origin("bloom"), CountryOfOrigin::Germany);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_unknown_model_origin() {
        assert_eq!(lookup_origin("some-random-model"), CountryOfOrigin::Unknown);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_lookup_is_case_insensitive() {
        assert_eq!(lookup_origin("Qwen:7B"), CountryOfOrigin::China);
        assert_eq!(
            lookup_origin("LLAMA3:latest"),
            CountryOfOrigin::UnitedStates
        );
        assert_eq!(lookup_origin("DeepSeek-R1"), CountryOfOrigin::China);
    }

    // rtmx:req REQ-LLM-043
    #[test]
    fn test_empty_model_is_unknown() {
        assert_eq!(lookup_origin(""), CountryOfOrigin::Unknown);
    }

    // --- REQ-LLM-044: Enums and default policy ---

    // rtmx:req REQ-LLM-044
    #[test]
    fn test_country_to_tier_default_policy() {
        let policy = ModelOriginPolicy::default();
        assert_eq!(
            policy.tier_for(CountryOfOrigin::UnitedStates),
            OriginTier::Approved
        );
        assert_eq!(
            policy.tier_for(CountryOfOrigin::France),
            OriginTier::Approved
        );
        assert_eq!(
            policy.tier_for(CountryOfOrigin::UnitedArabEmirates),
            OriginTier::Approved
        );
        assert_eq!(
            policy.tier_for(CountryOfOrigin::Germany),
            OriginTier::Approved
        );
        assert_eq!(policy.tier_for(CountryOfOrigin::China), OriginTier::Denied);
        assert_eq!(
            policy.tier_for(CountryOfOrigin::Unknown),
            OriginTier::Denied
        );
    }

    // rtmx:req REQ-LLM-044
    #[test]
    fn test_origin_tier_display() {
        assert_eq!(format!("{}", OriginTier::Approved), "approved");
        assert_eq!(format!("{}", OriginTier::Denied), "denied");
        assert_eq!(format!("{}", OriginTier::ReviewRequired), "review_required");
    }

    // rtmx:req REQ-LLM-044
    #[test]
    fn test_country_display() {
        assert_eq!(format!("{}", CountryOfOrigin::UnitedStates), "US");
        assert_eq!(format!("{}", CountryOfOrigin::China), "China");
        assert_eq!(format!("{}", CountryOfOrigin::Unknown), "Unknown");
    }

    // rtmx:req REQ-LLM-044
    #[test]
    fn test_evaluate_approved_model() {
        let policy = ModelOriginPolicy::default();
        let decision = policy.evaluate("llama3:latest");
        assert!(decision.is_allowed());
        assert_eq!(decision.origin, CountryOfOrigin::UnitedStates);
        assert_eq!(decision.tier, OriginTier::Approved);
    }

    // rtmx:req REQ-LLM-044
    #[test]
    fn test_evaluate_denied_model() {
        let policy = ModelOriginPolicy::default();
        let decision = policy.evaluate("qwen:7b");
        assert!(!decision.is_allowed());
        assert_eq!(decision.origin, CountryOfOrigin::China);
        assert_eq!(decision.tier, OriginTier::Denied);
        assert!(decision.reason.contains("restricted"));
        assert!(decision.reason.contains("China"));
    }

    // rtmx:req REQ-LLM-044
    #[test]
    fn test_evaluate_deepseek_denied() {
        let policy = ModelOriginPolicy::default();
        let decision = policy.evaluate("deepseek-r1:8b");
        assert!(!decision.is_allowed());
        assert_eq!(decision.tier, OriginTier::Denied);
    }

    // --- REQ-LLM-047: Policy overrides ---

    // rtmx:req REQ-LLM-047
    #[test]
    fn test_custom_policy_overrides_default() {
        let mut overrides = HashMap::new();
        overrides.insert(CountryOfOrigin::UnitedArabEmirates, OriginTier::Denied);
        let policy = ModelOriginPolicy::with_overrides(overrides);

        // UAE now denied instead of approved
        assert_eq!(
            policy.tier_for(CountryOfOrigin::UnitedArabEmirates),
            OriginTier::Denied
        );
        // Others unchanged
        assert_eq!(
            policy.tier_for(CountryOfOrigin::UnitedStates),
            OriginTier::Approved
        );
        assert_eq!(policy.tier_for(CountryOfOrigin::China), OriginTier::Denied);
    }

    // rtmx:req REQ-LLM-047
    #[test]
    fn test_override_can_approve_normally_denied() {
        let mut overrides = HashMap::new();
        overrides.insert(CountryOfOrigin::China, OriginTier::ReviewRequired);
        let policy = ModelOriginPolicy::with_overrides(overrides);

        let decision = policy.evaluate("qwen:7b");
        assert!(decision.is_allowed());
        assert_eq!(decision.tier, OriginTier::ReviewRequired);
    }

    // --- REQ-LLM-048: Default-deny for unclassified models ---

    // rtmx:req REQ-LLM-048
    #[test]
    fn test_unknown_model_denied_by_default() {
        let policy = ModelOriginPolicy::default();
        let decision = policy.evaluate("some-novel-model");
        assert!(!decision.is_allowed());
        assert_eq!(decision.origin, CountryOfOrigin::Unknown);
        assert_eq!(decision.tier, OriginTier::Denied);
    }

    // rtmx:req REQ-LLM-048
    #[test]
    fn test_allow_unclassified_promotes_to_review() {
        let policy = ModelOriginPolicy::default().allow_unclassified();
        let decision = policy.evaluate("some-novel-model");
        assert!(decision.is_allowed());
        assert_eq!(decision.tier, OriginTier::ReviewRequired);
    }

    // rtmx:req REQ-LLM-048
    #[test]
    fn test_allow_unclassified_does_not_affect_china() {
        let policy = ModelOriginPolicy::default().allow_unclassified();
        let decision = policy.evaluate("deepseek-r1:8b");
        assert!(!decision.is_allowed());
        assert_eq!(decision.tier, OriginTier::Denied);
    }

    // ---------- REQ-TUI-110: Air-gapped model manifest ----------

    // rtmx:req REQ-TUI-110
    #[test]
    fn test_airgap_manifest_parse() {
        let toml = r#"
[[models]]
name = "llama3:8b"
sha256 = "abc123def456"

[[models]]
name = "gemma4:2b"
sha256 = "789012345678"
"#;
        let manifest = ModelManifest::from_toml(toml).unwrap();
        assert_eq!(manifest.models.len(), 2);
        assert_eq!(manifest.models[0].name, "llama3:8b");
        assert_eq!(manifest.models[0].sha256, "abc123def456");
        assert_eq!(manifest.models[1].name, "gemma4:2b");
    }

    // rtmx:req REQ-TUI-110
    #[test]
    fn test_airgap_manifest_contains() {
        let manifest = ModelManifest {
            models: vec![
                ManifestEntry {
                    name: "llama3:8b".to_string(),
                    sha256: "abc".to_string(),
                },
                ManifestEntry {
                    name: "gemma4:2b".to_string(),
                    sha256: "def".to_string(),
                },
            ],
        };
        assert!(manifest.contains("llama3:8b"));
        assert!(manifest.contains("gemma4:2b"));
        assert!(!manifest.contains("qwen:7b"));
    }

    // rtmx:req REQ-TUI-110
    #[test]
    fn test_airgap_manifest_filter_models() {
        let manifest = ModelManifest {
            models: vec![ManifestEntry {
                name: "llama3:8b".to_string(),
                sha256: "abc".to_string(),
            }],
        };
        let all_models = ["llama3:8b", "gemma4:2b", "qwen:7b"];
        let filtered: Vec<&&str> = all_models.iter().filter(|m| manifest.contains(m)).collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(*filtered[0], "llama3:8b");
    }

    // rtmx:req REQ-TUI-110
    #[test]
    fn test_airgap_manifest_empty_toml() {
        let manifest = ModelManifest::from_toml("").unwrap();
        assert!(manifest.models.is_empty());
    }

    // rtmx:req REQ-TUI-110
    #[test]
    fn test_airgap_manifest_invalid_toml() {
        let result = ModelManifest::from_toml("not valid { toml");
        assert!(result.is_err());
    }
}
