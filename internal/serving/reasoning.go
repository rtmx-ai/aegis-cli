package serving

// Difficulty selects how much reasoning a task gets (THINK-001). A small local
// model pays a heavy latency tax for long reasoning and can even lose accuracy
// below ~10B, so reasoning is OFF by default and enabled only for hard tasks.
type Difficulty int

const (
	// Simple tasks never reason (the default).
	Simple Difficulty = iota
	// Hard tasks may reason when the calibration enables it.
	Hard
)

// Reasoning is the calibrated reasoning budget: whether the model reasons on hard
// tasks, and a token cap when it does. Zero value = reasoning off, no cap — the
// small-model default.
type Reasoning struct {
	// EnableForHard turns reasoning on for Hard tasks (Simple is always off).
	EnableForHard bool `json:"enable_for_hard,omitempty"`
	// MaxTokens caps reasoning tokens when enabled (0 = model default).
	MaxTokens int `json:"max_tokens,omitempty"`
}

// ReasoningBudget resolves (think, maxTokens) for a task difficulty (THINK-001):
// OFF by default, enabled only for Hard tasks when the calibration allows it.
// Simple tasks never reason, regardless of calibration.
func (c *Calibration) ReasoningBudget(d Difficulty) (think bool, maxTokens int) {
	if d == Hard && c.Reasoning.EnableForHard {
		return true, c.Reasoning.MaxTokens
	}
	return false, 0
}
