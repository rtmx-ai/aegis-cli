//! Adversary review chain: an independent LLM-backed agent that classifies
//! each proposed tool call as `Low`/`Medium`/`High`/`Critical` risk, with a
//! pluggable enforcement policy and audit trail.
//!
//! Covers:
//! - REQ-SECURITY-011: spawn independent reviewer + risk classification
//! - REQ-SECURITY-012: enforcement modes (Off / Warn / Enforce{threshold})
//! - REQ-SECURITY-013: audit trail for assessments, decoupled from
//!   `aegis-audit` via the [`AdversaryAuditSink`] trait
//!
//! # Design
//!
//! The adversary runs in parallel to the main agent loop. It shares no state
//! with the primary reasoning provider. Its system prompt narrowly constrains
//! its task: classify, do not propose new actions.
//!
//! The provider is expected to return a structured response with three
//! labelled lines -- `RISK:`, `REASONING:`, and `INDICATORS:`. Anything else
//! is rejected as [`AdversaryError::ParseError`] so that a silent downgrade
//! never occurs. Callers that want best-effort semantics must catch the
//! error themselves and decide how to proceed.

use std::sync::Arc;

use aegis_domain::ports::{LlmProvider, Message, Role, StreamEvent, TokenStream};
use aegis_domain::types::ToolCall;
use async_trait::async_trait;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Risk level
// ---------------------------------------------------------------------------

/// Risk classification for a proposed tool call.
///
/// Ordered from lowest to highest. Comparison operators therefore express
/// threshold semantics:
///
/// ```
/// use aegis_security::adversary::RiskLevel;
/// assert!(RiskLevel::Critical > RiskLevel::High);
/// assert!(RiskLevel::Low < RiskLevel::Medium);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// Read-only operations on safe paths.
    Low,
    /// Writes to project files.
    Medium,
    /// Writes to system locations, network operations, package installs.
    High,
    /// Destructive operations (rm -rf, dd, fork bombs), credential access,
    /// data exfiltration.
    Critical,
}

impl RiskLevel {
    fn parse(label: &str) -> Option<Self> {
        match label.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" | "med" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" | "crit" => Some(Self::Critical),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Assessment + errors
// ---------------------------------------------------------------------------

/// A completed risk assessment produced by the adversary.
#[derive(Debug, Clone)]
pub struct RiskAssessment {
    /// The original tool call under review.
    pub tool_call: ToolCall,
    /// Classified risk level.
    pub risk: RiskLevel,
    /// One-paragraph justification.
    pub reasoning: String,
    /// Specific patterns flagged (e.g., "rm -rf", "network", "etc").
    pub indicators: Vec<String>,
}

// `ToolCall` in the shared kernel does not derive `PartialEq`, so we
// hand-roll structural equality on `RiskAssessment` by comparing the
// `Debug`-formatted tool call alongside the descriptive fields. This is
// sufficient for tests and audit-trail introspection without widening the
// domain kernel's trait surface.
impl PartialEq for RiskAssessment {
    fn eq(&self, other: &Self) -> bool {
        self.risk == other.risk
            && self.reasoning == other.reasoning
            && self.indicators == other.indicators
            && format!("{:?}", self.tool_call) == format!("{:?}", other.tool_call)
    }
}

impl Eq for RiskAssessment {}

/// Errors produced by the adversary.
#[derive(Debug, Error)]
pub enum AdversaryError {
    /// The underlying LLM provider failed.
    #[error("adversary provider error: {0}")]
    ProviderError(String),
    /// The provider response did not match the expected structured format.
    #[error("adversary parse error: {0}")]
    ParseError(String),
}

// ---------------------------------------------------------------------------
// Enforcement mode + decision
// ---------------------------------------------------------------------------

/// Enforcement policy applied to an [`AdversaryAgent`] assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementMode {
    /// Adversary is disabled. No risk assessment runs.
    Off,
    /// Adversary classifies risk but never blocks. Logs to audit ledger.
    Warn,
    /// Adversary blocks tool calls at or above the configured threshold.
    Enforce {
        /// Inclusive threshold. Assessments with `risk >= threshold` block.
        threshold: RiskLevel,
    },
}

/// Final decision emitted by [`AdversaryAgent::evaluate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnforcementDecision {
    /// Tool call may proceed (Off, Warn, or Enforce below threshold).
    Allow {
        /// `None` only when the mode was [`EnforcementMode::Off`] so no
        /// classification was performed. Otherwise an assessment is present.
        assessment: Option<RiskAssessment>,
    },
    /// Tool call is blocked. `assessment.risk >= threshold` under
    /// [`EnforcementMode::Enforce`].
    Block { assessment: RiskAssessment },
}

// ---------------------------------------------------------------------------
// Audit sink
// ---------------------------------------------------------------------------

/// Pluggable audit hook for the adversary.
///
/// Implementors persist assessments into whatever storage they manage
/// (typically `aegis-audit::JsonlLedger`). This trait lives here rather than
/// in `aegis-audit` so that `aegis-security` does not take a dependency on
/// `aegis-audit` -- that would introduce a cycle at the workspace level.
#[async_trait]
pub trait AdversaryAuditSink: Send + Sync {
    /// Record a risk assessment to the audit trail.
    ///
    /// The sink receives both the decision (which contains the assessment
    /// when available) and the enforcement mode used, so that downstream
    /// consumers can reconstruct the full review context.
    async fn record_assessment(&self, decision: &EnforcementDecision, mode: EnforcementMode);
}

// ---------------------------------------------------------------------------
// Adversary agent
// ---------------------------------------------------------------------------

const SYSTEM_PROMPT: &str = "\
You are a security review agent. Your only job is to classify proposed tool \
calls as Low, Medium, High, or Critical risk.\n\
\n\
Risk levels:\n\
- Low: read-only operations on safe paths.\n\
- Medium: writes to project files.\n\
- High: writes to system locations, network operations, package installs.\n\
- Critical: destructive operations (rm -rf, dd, fork bombs), credential \
access, data exfiltration.\n\
\n\
You MUST respond with exactly this structure, one field per line:\n\
RISK: <Low|Medium|High|Critical>\n\
REASONING: <one paragraph>\n\
INDICATORS: <comma-separated list of specific patterns you flagged>\n\
\n\
Do not propose alternatives. Do not call tools. Do not output anything outside \
these three labelled lines.\
";

/// Independent reviewer agent that classifies tool calls.
///
/// The provider is stored behind an `Arc` so the same reviewer can be held
/// by the main loop and by background tasks without cloning the underlying
/// HTTP client.
pub struct AdversaryAgent {
    provider: Arc<dyn LlmProvider>,
    system_prompt: String,
}

impl AdversaryAgent {
    /// Construct with the default system prompt.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            system_prompt: SYSTEM_PROMPT.to_string(),
        }
    }

    /// Override the adversary's system prompt. Intended for tests or
    /// customer-authored review policies; production deployments should
    /// stick with the built-in prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Classify the risk of a proposed tool call.
    ///
    /// Sends the system prompt followed by the supplied `context` (the main
    /// agent's recent messages), then a user turn describing the tool call.
    /// The provider's response is parsed into a [`RiskAssessment`].
    pub async fn classify(
        &self,
        tool_call: &ToolCall,
        context: &[Message],
    ) -> Result<RiskAssessment, AdversaryError> {
        let mut messages: Vec<Message> = Vec::with_capacity(context.len() + 2);
        messages.push(Message {
            role: Role::System,
            content: self.system_prompt.clone(),
            cache_control: None,
        });
        messages.extend(context.iter().cloned());
        messages.push(Message {
            role: Role::User,
            content: format!(
                "Classify the following tool call. Respond only in the \
                 RISK/REASONING/INDICATORS format.\n\nTOOL_CALL: {tool_call:?}"
            ),
            cache_control: None,
        });

        let mut stream = self
            .provider
            .stream(&messages, &[])
            .await
            .map_err(|e| AdversaryError::ProviderError(e.to_string()))?;

        let body = drain_stream(stream.as_mut()).await?;
        let (risk, reasoning, indicators) = parse_response(&body)?;
        Ok(RiskAssessment {
            tool_call: tool_call.clone(),
            risk,
            reasoning,
            indicators,
        })
    }

    /// Classify under an [`EnforcementMode`] and return an
    /// [`EnforcementDecision`].
    ///
    /// `Off` returns `Allow { assessment: None }` without touching the
    /// provider. `Warn` always returns `Allow` but with the assessment
    /// attached. `Enforce { threshold }` blocks iff `assessment.risk >=
    /// threshold`.
    pub async fn evaluate(
        &self,
        tool_call: &ToolCall,
        context: &[Message],
        mode: EnforcementMode,
    ) -> Result<EnforcementDecision, AdversaryError> {
        if matches!(mode, EnforcementMode::Off) {
            return Ok(EnforcementDecision::Allow { assessment: None });
        }

        let assessment = self.classify(tool_call, context).await?;

        Ok(match mode {
            EnforcementMode::Off => unreachable!("handled above"),
            EnforcementMode::Warn => EnforcementDecision::Allow {
                assessment: Some(assessment),
            },
            EnforcementMode::Enforce { threshold } => {
                if assessment.risk >= threshold {
                    EnforcementDecision::Block { assessment }
                } else {
                    EnforcementDecision::Allow {
                        assessment: Some(assessment),
                    }
                }
            }
        })
    }

    /// Evaluate then record the decision to the provided audit sink.
    ///
    /// In [`EnforcementMode::Off`] the sink is NOT called, because no
    /// classification occurred -- there is nothing to record. This matches
    /// REQ-SECURITY-013: the audit trail carries adversary assessments, not
    /// "adversary disabled" markers. Deployments that want to audit the
    /// absence of review should record that at configuration time.
    pub async fn evaluate_and_record(
        &self,
        tool_call: &ToolCall,
        context: &[Message],
        mode: EnforcementMode,
        sink: &dyn AdversaryAuditSink,
    ) -> Result<EnforcementDecision, AdversaryError> {
        let decision = self.evaluate(tool_call, context, mode).await?;
        if !matches!(mode, EnforcementMode::Off) {
            sink.record_assessment(&decision, mode).await;
        }
        Ok(decision)
    }
}

// ---------------------------------------------------------------------------
// Stream + response parsing
// ---------------------------------------------------------------------------

async fn drain_stream(stream: &mut dyn TokenStream) -> Result<String, AdversaryError> {
    let mut body = String::new();
    while let Some(evt) = stream.next().await {
        match evt {
            StreamEvent::Token(t) => body.push_str(&t),
            StreamEvent::Done { .. } => break,
            StreamEvent::Error(msg) => return Err(AdversaryError::ProviderError(msg)),
            StreamEvent::RetryableError { message, .. } => {
                return Err(AdversaryError::ProviderError(message));
            }
            // The adversary never calls tools itself; if the provider
            // attempts to invoke one we treat it as a protocol violation.
            StreamEvent::ToolUse(_) => {
                return Err(AdversaryError::ParseError(
                    "adversary provider must not emit ToolUse events".into(),
                ));
            }
        }
    }
    Ok(body)
}

fn parse_response(body: &str) -> Result<(RiskLevel, String, Vec<String>), AdversaryError> {
    let mut risk: Option<RiskLevel> = None;
    let mut reasoning: Option<String> = None;
    let mut indicators: Option<Vec<String>> = None;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if let Some(rest) = strip_prefix_ci(line, "RISK:") {
            risk = RiskLevel::parse(rest);
        } else if let Some(rest) = strip_prefix_ci(line, "REASONING:") {
            reasoning = Some(rest.trim().to_string());
        } else if let Some(rest) = strip_prefix_ci(line, "INDICATORS:") {
            let parts: Vec<String> = rest
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            indicators = Some(parts);
        }
    }

    match (risk, reasoning, indicators) {
        (Some(r), Some(reason), Some(ind)) if !reason.is_empty() && !ind.is_empty() => {
            Ok((r, reason, ind))
        }
        _ => Err(AdversaryError::ParseError(format!(
            "expected `RISK: ...`, `REASONING: ...`, `INDICATORS: ...`, got: {body:?}"
        ))),
    }
}

fn strip_prefix_ci<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&line[prefix.len()..])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Unit tests for pure parsing helpers.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-SECURITY-011
    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    // rtmx:req REQ-SECURITY-011
    #[test]
    fn parse_response_happy_path() {
        let body = "RISK: High\nREASONING: writes /etc\nINDICATORS: write_file, etc";
        let (risk, reason, indicators) = parse_response(body).unwrap();
        assert_eq!(risk, RiskLevel::High);
        assert_eq!(reason, "writes /etc");
        assert_eq!(
            indicators,
            vec!["write_file".to_string(), "etc".to_string()]
        );
    }

    // rtmx:req REQ-SECURITY-011
    #[test]
    fn parse_response_case_insensitive() {
        let body = "risk: critical\nreasoning: rm -rf\nindicators: rm";
        let (risk, _reason, _ind) = parse_response(body).unwrap();
        assert_eq!(risk, RiskLevel::Critical);
    }

    // rtmx:req REQ-SECURITY-011
    #[test]
    fn parse_response_missing_fields_errors() {
        let body = "RISK: Low\nINDICATORS: x";
        assert!(matches!(
            parse_response(body),
            Err(AdversaryError::ParseError(_))
        ));
    }

    // rtmx:req REQ-SECURITY-011
    #[test]
    fn parse_response_unknown_label_errors() {
        let body = "RISK: wonky\nREASONING: x\nINDICATORS: y";
        assert!(matches!(
            parse_response(body),
            Err(AdversaryError::ParseError(_))
        ));
    }

    // rtmx:req REQ-SECURITY-012
    #[test]
    fn enforce_threshold_semantics() {
        assert!(RiskLevel::Critical >= RiskLevel::High);
        assert!(RiskLevel::High >= RiskLevel::High);
        assert!(RiskLevel::Medium < RiskLevel::High);
    }
}
