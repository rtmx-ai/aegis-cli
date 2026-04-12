//! Interactive first-run tutorial for new users.
//!
//! Detects whether this is the user's first run (no config file exists)
//! and defines a structured walkthrough of aegis setup:
//! 1. Mode selection (local / cloud / air-gapped)
//! 2. Backend configuration (detected providers or manual entry)
//! 3. Quick connectivity test
//!
//! The tutorial steps are data-driven so the TUI can render them
//! without coupling to this module's internals.

use std::io;
use std::path::Path;

/// Marker file written after the tutorial completes.
const TUTORIAL_COMPLETE_MARKER: &str = ".tutorial_complete";

/// A single step in the first-run tutorial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TutorialStep {
    /// Machine-readable step identifier.
    pub id: &'static str,
    /// Human-readable title shown in the TUI header.
    pub title: &'static str,
    /// Longer description shown in the TUI body.
    pub description: &'static str,
}

/// Tracks progress through the tutorial steps.
#[derive(Debug, Clone)]
pub struct TutorialState {
    /// The ordered list of steps.
    pub steps: Vec<TutorialStep>,
    /// Index of the current step (0-based).
    pub current_step: usize,
}

impl TutorialState {
    /// Create a new tutorial state with the standard step sequence.
    pub fn new() -> Self {
        Self {
            steps: run_tutorial_steps(),
            current_step: 0,
        }
    }

    /// Advance to the next step. Returns `true` if there are more
    /// steps, `false` if the tutorial is complete.
    pub fn advance(&mut self) -> bool {
        if self.current_step + 1 < self.steps.len() {
            self.current_step += 1;
            true
        } else {
            false
        }
    }

    /// Returns the current step, or `None` if all steps are done.
    pub fn current(&self) -> Option<&TutorialStep> {
        self.steps.get(self.current_step)
    }

    /// Returns `true` when the user has completed all steps.
    pub fn is_complete(&self) -> bool {
        self.current_step >= self.steps.len()
    }
}

impl Default for TutorialState {
    fn default() -> Self {
        Self::new()
    }
}

/// Check whether this is a first run (no config file exists).
///
/// Returns `true` when the config directory does not contain a
/// `config.yaml` file, indicating the user has never run `aegis init`.
pub fn is_first_run(config_dir: &Path) -> bool {
    !config_dir.join("config.yaml").exists()
}

/// Return the welcome message shown at the start of the tutorial.
pub fn welcome_message() -> &'static str {
    "Welcome to aegis -- a terminal-native agentic AI pair programmer \
     for CUI environments.\n\n\
     This tutorial will walk you through initial setup so you can \
     start using aegis in a few minutes. You will:\n\
     1. Choose a deployment mode\n\
     2. Configure your LLM backend\n\
     3. Verify connectivity\n\n\
     Let's get started."
}

/// Return the ordered list of tutorial steps.
///
/// The TUI renders these sequentially; each step collects user input
/// or runs an automated check before advancing.
pub fn run_tutorial_steps() -> Vec<TutorialStep> {
    vec![
        TutorialStep {
            id: "mode_selection",
            title: "Select deployment mode",
            description: "Choose how aegis connects to an LLM backend:\n\
                 - local: Air-gapped operation with a local model (Ollama, vLLM)\n\
                 - cloud: Connect to a GovCloud endpoint (Vertex AI, Bedrock)\n\
                 - air-gapped: Fully offline with a bundled model",
        },
        TutorialStep {
            id: "backend_config",
            title: "Configure LLM backend",
            description: "Specify (or auto-detect) the LLM endpoint and model.\n\
                 For local mode, aegis scans localhost for running providers.\n\
                 For cloud mode, enter your endpoint URL and credentials.",
        },
        TutorialStep {
            id: "connectivity_test",
            title: "Verify connectivity",
            description: "Send a minimal request to the configured endpoint to \
                 confirm it is reachable and responding. This does NOT send \
                 any sensitive data.",
        },
    ]
}

/// Write a marker file indicating the tutorial has been completed.
///
/// The marker is a zero-byte file at `<config_dir>/.tutorial_complete`.
/// Subsequent calls to [`is_first_run`] still check for `config.yaml`,
/// but callers can use [`is_tutorial_complete`] to skip the tutorial
/// even if config exists.
pub fn mark_tutorial_complete(config_dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let marker = config_dir.join(TUTORIAL_COMPLETE_MARKER);
    std::fs::write(&marker, "")?;
    Ok(())
}

/// Check whether the tutorial has already been completed.
pub fn is_tutorial_complete(config_dir: &Path) -> bool {
    config_dir.join(TUTORIAL_COMPLETE_MARKER).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn is_first_run_returns_true_when_no_config() {
        let tmp = TempDir::new().unwrap();
        assert!(
            is_first_run(tmp.path()),
            "Should be first run when no config.yaml exists"
        );
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn is_first_run_returns_false_when_config_exists() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("config.yaml"), "version: '1.0'\n").unwrap();
        assert!(
            !is_first_run(tmp.path()),
            "Should NOT be first run when config.yaml exists"
        );
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn tutorial_steps_returns_three_steps() {
        let steps = run_tutorial_steps();
        assert_eq!(steps.len(), 3, "Tutorial should have exactly 3 steps");
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn tutorial_steps_have_correct_ids() {
        let steps = run_tutorial_steps();
        assert_eq!(steps[0].id, "mode_selection");
        assert_eq!(steps[1].id, "backend_config");
        assert_eq!(steps[2].id, "connectivity_test");
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn tutorial_state_advances_through_steps() {
        let mut state = TutorialState::new();
        assert_eq!(state.current_step, 0);
        assert!(!state.is_complete());

        assert!(state.advance(), "Should advance to step 1");
        assert_eq!(state.current_step, 1);

        assert!(state.advance(), "Should advance to step 2");
        assert_eq!(state.current_step, 2);

        assert!(!state.advance(), "Should NOT advance past last step");
        assert_eq!(state.current_step, 2);
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn tutorial_state_current_returns_correct_step() {
        let mut state = TutorialState::new();
        assert_eq!(state.current().unwrap().id, "mode_selection");
        state.advance();
        assert_eq!(state.current().unwrap().id, "backend_config");
        state.advance();
        assert_eq!(state.current().unwrap().id, "connectivity_test");
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn mark_tutorial_complete_writes_marker() {
        let tmp = TempDir::new().unwrap();
        assert!(
            !is_tutorial_complete(tmp.path()),
            "Should not be complete before marking"
        );

        mark_tutorial_complete(tmp.path()).unwrap();
        assert!(
            is_tutorial_complete(tmp.path()),
            "Should be complete after marking"
        );
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn mark_tutorial_complete_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("deep").join("nested");
        mark_tutorial_complete(&nested).unwrap();
        assert!(is_tutorial_complete(&nested));
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn mark_tutorial_complete_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        mark_tutorial_complete(tmp.path()).unwrap();
        mark_tutorial_complete(tmp.path()).unwrap();
        assert!(is_tutorial_complete(tmp.path()));
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn welcome_message_is_non_empty() {
        let msg = welcome_message();
        assert!(!msg.is_empty(), "Welcome message should not be empty");
        assert!(
            msg.contains("aegis"),
            "Welcome message should mention aegis"
        );
    }

    // rtmx:req REQ-ONBOARD-011
    #[test]
    fn tutorial_state_default_matches_new() {
        let from_new = TutorialState::new();
        let from_default = TutorialState::default();
        assert_eq!(from_new.steps, from_default.steps);
        assert_eq!(from_new.current_step, from_default.current_step);
    }
}
