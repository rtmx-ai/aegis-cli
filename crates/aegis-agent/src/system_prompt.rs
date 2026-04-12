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
