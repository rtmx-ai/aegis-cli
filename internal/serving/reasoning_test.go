package serving

import "testing"

// TestReasoningBudget → REQ-THINK-001: the reasoning budget is a calibration param,
// OFF by default; reasoning is enabled only for Hard tasks when the calibration
// allows it, and Simple tasks never reason.
func TestReasoningBudget(t *testing.T) {
	// Off by default (zero-value calibration): neither difficulty reasons.
	c := &Calibration{}
	if th, _ := c.ReasoningBudget(Simple); th {
		t.Error("simple must have reasoning off by default")
	}
	if th, _ := c.ReasoningBudget(Hard); th {
		t.Error("hard must have reasoning off by default (not enabled)")
	}

	// Calibrated to reason on hard tasks with a token cap.
	c.Reasoning = Reasoning{EnableForHard: true, MaxTokens: 2048}
	if th, mt := c.ReasoningBudget(Hard); !th || mt != 2048 {
		t.Errorf("hard+enabled: want think=true max=2048, got think=%v max=%d", th, mt)
	}
	// Simple stays off even when hard reasoning is enabled.
	if th, _ := c.ReasoningBudget(Simple); th {
		t.Error("simple must stay off even when hard reasoning is enabled")
	}
}
