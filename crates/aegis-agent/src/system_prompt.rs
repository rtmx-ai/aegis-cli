//! System prompt management with layered priority.
//!
//! The agent needs a system prompt composed from multiple sources: a base
//! prompt shipped with the binary, project-level overrides (e.g., from
//! `.aegis/system_prompt`), and session-level overrides (e.g., user input
//! at runtime). Higher-priority layers override or extend lower ones.
//!
//! Layers are concatenated in ascending priority order (Base, then Project,
//! then Session), separated by double newlines. Missing layers are skipped.

use std::collections::BTreeMap;

/// Base system prompt shipped with the aegis binary (REQ-AGENT-042).
///
/// Teaches the model its identity, available tools, security posture,
/// RTMX awareness, and behavioral rules. Kept under 1500 tokens for
/// compatibility with 8B-class local models.
pub const BASE_SYSTEM_PROMPT: &str = "\
You are aegis, a terminal-native AI pair programmer built for software engineers \
in defense and critical infrastructure environments. You operate inside a TUI \
with human-in-the-loop (HITL) approval for all state-mutating actions.

# Tools

You have these tools. Use them to accomplish tasks:

- read_file <path>: Read file contents. Safe, executes automatically.
- write_file <path> <content>: Write to a file. Requires HITL approval.
- run_command <cmd>: Execute a shell command. Requires HITL approval.
- list_dir <path>: List directory contents. Safe, executes automatically.
- grep <pattern> [path]: Search file contents. Safe, executes automatically.

Safe tools (read_file, list_dir, grep) execute without approval. \
Mutating tools (write_file, run_command) are blocked until the user explicitly \
approves via the HITL gate. Never claim you executed a tool without actually \
calling it.

# Security

- Never include file contents, secrets, or CUI (Controlled Unclassified Information) \
in your reasoning or responses beyond what is necessary.
- Respect .aegisignore: files matching blocked patterns (*.pem, *.key, .env, etc.) \
are inaccessible. Do not attempt to read them.
- If you encounter content marked CUI, FOUO, or with distribution statements, \
do not reproduce it. Summarize or reference it by path only.

# RTMX Requirements

This project may track requirements in .rtmx/database.csv. Each requirement has \
an ID (e.g., REQ-AGENT-035), status, test linkage, and dependencies. When working \
on a requirement:
1. Read the requirement and its dependencies first.
2. Write a failing test before implementation (TDD).
3. Mark tests with // rtmx:req REQ-XXX-NNN comments.
4. Do not mark a requirement complete without passing tests.

# Behavior

- Be direct. Lead with the answer or action, not reasoning.
- No emojis. Text only.
- When modifying code, read the file first. Understand before changing.
- Prefer minimal, targeted changes over broad refactoring.
- If a task is ambiguous, ask for clarification rather than guessing.";

/// Priority layer for a system prompt fragment.
///
/// Variants are ordered by ascending priority: Base < Project < Session.
/// Higher-priority layers appear later in the assembled prompt, allowing
/// them to refine or override earlier instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemPromptLayer {
    /// Default prompt shipped with the aegis binary.
    Base = 0,
    /// Project-level override (e.g., `.aegis/system_prompt`).
    Project = 1,
    /// Session-level override (e.g., user-supplied at runtime).
    Session = 2,
}

/// Manages layered system prompt fragments and assembles them into a
/// single prompt string for LLM dispatch.
#[derive(Debug, Clone, Default)]
pub struct SystemPromptManager {
    layers: BTreeMap<SystemPromptLayer, String>,
}

impl SystemPromptManager {
    /// Create a new, empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a manager pre-loaded with the aegis base system prompt.
    pub fn with_base() -> Self {
        let mut mgr = Self::new();
        mgr.set_layer(SystemPromptLayer::Base, BASE_SYSTEM_PROMPT);
        mgr
    }

    /// Set the prompt text for a given layer, replacing any previous value.
    pub fn set_layer(&mut self, layer: SystemPromptLayer, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            self.layers.remove(&layer);
        } else {
            self.layers.insert(layer, text);
        }
    }

    /// Remove a layer entirely.
    pub fn clear_layer(&mut self, layer: SystemPromptLayer) {
        self.layers.remove(&layer);
    }

    /// Returns true if no layers have been set.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Build the final system prompt by concatenating layers in priority
    /// order (Base, then Project, then Session), separated by double
    /// newlines. Returns an empty string if no layers are set.
    pub fn build(&self) -> String {
        self.layers
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-AGENT-042
    #[test]
    fn test_base_prompt_contains_identity_and_tools() {
        let prompt = BASE_SYSTEM_PROMPT;
        // Identity
        assert!(prompt.contains("aegis"), "must identify as aegis");
        assert!(prompt.contains("pair programmer"), "must describe role");
        // Tools with risk levels
        assert!(prompt.contains("read_file"), "must list read_file");
        assert!(prompt.contains("write_file"), "must list write_file");
        assert!(prompt.contains("run_command"), "must list run_command");
        assert!(prompt.contains("HITL"), "must mention HITL gates");
        // Security posture
        assert!(prompt.contains("CUI"), "must mention CUI");
        assert!(prompt.contains(".aegisignore"), "must mention aegisignore");
        // RTMX awareness
        assert!(prompt.contains("rtmx:req"), "must mention test markers");
        // Behavioral rules
        assert!(prompt.contains("No emojis"), "must enforce no emojis");
    }

    // rtmx:req REQ-AGENT-042
    #[test]
    fn test_with_base_constructor_sets_base_layer() {
        let mgr = SystemPromptManager::with_base();
        assert!(!mgr.is_empty());
        let prompt = mgr.build();
        assert!(prompt.contains("You are aegis"));
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn empty_manager_builds_empty_string() {
        let mgr = SystemPromptManager::new();
        assert_eq!(mgr.build(), "");
        assert!(mgr.is_empty());
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn single_base_layer() {
        let mut mgr = SystemPromptManager::new();
        mgr.set_layer(SystemPromptLayer::Base, "You are aegis.");
        assert_eq!(mgr.build(), "You are aegis.");
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn layers_concatenated_in_priority_order() {
        let mut mgr = SystemPromptManager::new();
        // Insert out of order to verify sorting.
        mgr.set_layer(SystemPromptLayer::Session, "Focus on security.");
        mgr.set_layer(SystemPromptLayer::Base, "You are aegis.");
        mgr.set_layer(SystemPromptLayer::Project, "This project uses Rust.");

        let prompt = mgr.build();
        let parts: Vec<&str> = prompt.split("\n\n").collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "You are aegis.");
        assert_eq!(parts[1], "This project uses Rust.");
        assert_eq!(parts[2], "Focus on security.");
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn higher_layer_replaces_previous_value() {
        let mut mgr = SystemPromptManager::new();
        mgr.set_layer(SystemPromptLayer::Project, "old instructions");
        mgr.set_layer(SystemPromptLayer::Project, "new instructions");
        assert_eq!(mgr.build(), "new instructions");
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn clear_layer_removes_it() {
        let mut mgr = SystemPromptManager::new();
        mgr.set_layer(SystemPromptLayer::Base, "You are aegis.");
        mgr.set_layer(SystemPromptLayer::Project, "This project uses Rust.");
        mgr.clear_layer(SystemPromptLayer::Project);
        assert_eq!(mgr.build(), "You are aegis.");
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn setting_empty_text_removes_layer() {
        let mut mgr = SystemPromptManager::new();
        mgr.set_layer(SystemPromptLayer::Base, "You are aegis.");
        mgr.set_layer(SystemPromptLayer::Base, "");
        assert!(mgr.is_empty());
        assert_eq!(mgr.build(), "");
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn skips_missing_middle_layer() {
        let mut mgr = SystemPromptManager::new();
        mgr.set_layer(SystemPromptLayer::Base, "base prompt");
        mgr.set_layer(SystemPromptLayer::Session, "session override");
        let prompt = mgr.build();
        assert_eq!(prompt, "base prompt\n\nsession override");
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn layer_ordering_matches_enum_variants() {
        assert!(SystemPromptLayer::Base < SystemPromptLayer::Project);
        assert!(SystemPromptLayer::Project < SystemPromptLayer::Session);
    }

    // rtmx:req REQ-AGENT-015
    #[test]
    fn clone_produces_independent_copy() {
        let mut mgr = SystemPromptManager::new();
        mgr.set_layer(SystemPromptLayer::Base, "original");
        let mut cloned = mgr.clone();
        cloned.set_layer(SystemPromptLayer::Base, "modified");
        assert_eq!(mgr.build(), "original");
        assert_eq!(cloned.build(), "modified");
    }
}
