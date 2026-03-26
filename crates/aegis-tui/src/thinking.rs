//! Thinking animation with contextual verbs for the status line.
//!
//! When the agent is waiting for an LLM response, this module produces
//! animated status text that cycles through contextual verbs with a
//! dot progression (e.g. "Thinking.", "Thinking..", "Thinking...").

/// Contextual verbs displayed while the agent is thinking.
pub const THINKING_VERBS: &[&str] = &[
    "Thinking",
    "Reasoning",
    "Analyzing",
    "Considering",
    "Evaluating",
];

/// Animated status text that cycles through contextual verbs with dot
/// progression. Each tick advances the dot count (1..3), and every full
/// dot cycle advances to the next verb.
pub struct ThinkingAnimation {
    tick: usize,
}

impl ThinkingAnimation {
    /// Create a new animation starting at tick 0.
    pub fn new() -> Self {
        Self { tick: 0 }
    }

    /// Advance the animation by one frame.
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Return the current frame text, e.g. "Analyzing..".
    pub fn current_text(&self) -> String {
        let verb_index = (self.tick / 3) % THINKING_VERBS.len();
        let dot_count = (self.tick % 3) + 1;
        let dots: String = ".".repeat(dot_count);
        format!("{}{}", THINKING_VERBS[verb_index], dots)
    }

    /// Reset the animation to its initial state.
    pub fn reset(&mut self) {
        self.tick = 0;
    }
}

impl Default for ThinkingAnimation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-TUI-006
    #[test]
    fn new_animation_starts_at_first_verb_one_dot() {
        let anim = ThinkingAnimation::new();
        assert_eq!(anim.current_text(), "Thinking.");
    }

    // @req REQ-TUI-006
    #[test]
    fn tick_advances_dot_count() {
        let mut anim = ThinkingAnimation::new();
        assert_eq!(anim.current_text(), "Thinking.");
        anim.tick();
        assert_eq!(anim.current_text(), "Thinking..");
        anim.tick();
        assert_eq!(anim.current_text(), "Thinking...");
    }

    // @req REQ-TUI-006
    #[test]
    fn verb_changes_after_three_ticks() {
        let mut anim = ThinkingAnimation::new();
        // Ticks 0,1,2 = Thinking with 1,2,3 dots
        anim.tick(); // tick 1
        anim.tick(); // tick 2
        anim.tick(); // tick 3 -> next verb
        assert_eq!(anim.current_text(), "Reasoning.");
    }

    // @req REQ-TUI-006
    #[test]
    fn cycles_through_all_verbs() {
        let mut anim = ThinkingAnimation::new();
        let mut first_frames: Vec<String> = Vec::new();
        for _ in 0..THINKING_VERBS.len() {
            first_frames.push(anim.current_text());
            anim.tick();
            anim.tick();
            anim.tick();
        }
        assert_eq!(
            first_frames,
            vec![
                "Thinking.",
                "Reasoning.",
                "Analyzing.",
                "Considering.",
                "Evaluating.",
            ]
        );
    }

    // @req REQ-TUI-006
    #[test]
    fn wraps_back_to_first_verb_after_full_cycle() {
        let mut anim = ThinkingAnimation::new();
        // 5 verbs * 3 ticks each = 15 ticks for a full cycle
        for _ in 0..15 {
            anim.tick();
        }
        assert_eq!(anim.current_text(), "Thinking.");
    }

    // @req REQ-TUI-006
    #[test]
    fn reset_returns_to_initial_state() {
        let mut anim = ThinkingAnimation::new();
        anim.tick();
        anim.tick();
        anim.tick();
        anim.tick();
        assert_ne!(anim.current_text(), "Thinking.");
        anim.reset();
        assert_eq!(anim.current_text(), "Thinking.");
    }

    // @req REQ-TUI-006
    #[test]
    fn default_is_same_as_new() {
        let anim = ThinkingAnimation::default();
        assert_eq!(anim.current_text(), "Thinking.");
    }

    // @req REQ-TUI-006
    #[test]
    fn thinking_verbs_has_five_entries() {
        assert_eq!(THINKING_VERBS.len(), 5);
    }

    // @req REQ-TUI-006
    #[test]
    fn dot_progression_within_single_verb() {
        let mut anim = ThinkingAnimation::new();
        // Advance to "Analyzing" (verb index 2, tick 6)
        for _ in 0..6 {
            anim.tick();
        }
        assert_eq!(anim.current_text(), "Analyzing.");
        anim.tick();
        assert_eq!(anim.current_text(), "Analyzing..");
        anim.tick();
        assert_eq!(anim.current_text(), "Analyzing...");
    }
}
