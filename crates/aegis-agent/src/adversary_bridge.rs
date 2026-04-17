//! Adversary review bridge for the agent loop.
//!
//! Defines the port (`AdversaryReviewer`) that the agent loop uses to
//! request adversary risk assessments. The concrete implementation lives
//! in `aegis-security::adversary::AdversaryAgent`; the composition root
//! (`aegis-cli`) wires it in. This avoids a compile-time dependency from
//! `aegis-agent` to `aegis-security`.
//!
//! rtmx:req REQ-SECURITY-004

use aegis_domain::ports::Message;
use aegis_domain::types::ToolCall;
use async_trait::async_trait;
use std::fmt;

// ---------------------------------------------------------------------------
// Risk level (mirrors aegis_security::adversary::RiskLevel)
// ---------------------------------------------------------------------------

/// Risk classification for a proposed tool call.
///
/// This is a local mirror of the type in `aegis-security` so that
/// `aegis-agent` does not depend on `aegis-security` at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

// ---------------------------------------------------------------------------
// Assessment
// ---------------------------------------------------------------------------

/// A completed risk assessment produced by the adversary reviewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAssessment {
    /// Classified risk level.
    pub risk: RiskLevel,
    /// One-paragraph justification.
    pub reasoning: String,
    /// Specific patterns flagged.
    pub indicators: Vec<String>,
}

// ---------------------------------------------------------------------------
// Enforcement mode + decision
// ---------------------------------------------------------------------------

/// Enforcement policy for the adversary reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMode {
    /// Adversary is disabled. No risk assessment runs.
    Off,
    /// Adversary classifies risk but never blocks. Logs to audit.
    Warn,
    /// Adversary blocks tool calls at or above the configured threshold.
    Enforce {
        /// Inclusive threshold. Assessments with `risk >= threshold` block.
        threshold: RiskLevel,
    },
}

/// Final decision from the adversary reviewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewDecision {
    /// Tool call may proceed. Assessment is `None` when mode is `Off`.
    Allow {
        assessment: Option<ReviewAssessment>,
    },
    /// Tool call is blocked (risk >= threshold under `Enforce` mode).
    Block { assessment: ReviewAssessment },
}

// ---------------------------------------------------------------------------
// Port trait
// ---------------------------------------------------------------------------

/// Port for adversary risk review.
///
/// The agent loop calls `review()` before the HITL gate for every tool
/// call when an adversary is configured. Implementations are expected to
/// honour the `mode` parameter: `Off` returns `Allow { None }` without
/// invoking any LLM.
#[async_trait]
pub trait AdversaryReviewer: Send + Sync {
    /// Review a proposed tool call under the given enforcement mode.
    async fn review(
        &self,
        tool_call: &ToolCall,
        context: &[Message],
        mode: ReviewMode,
    ) -> Result<ReviewDecision, String>;
}
