package opencode

import (
	"strings"
	"testing"
)

// TestSequentialSubagents → REQ-LONGRUN-005: sub-agent delegation is sequential by
// policy (MaxConcurrent=1, bounded sub-tasks); a parallel policy is rejected.
func TestSequentialSubagents(t *testing.T) {
	p := DefaultSubagentPolicy()
	if p.MaxConcurrent != 1 {
		t.Errorf("default delegation must be sequential, got MaxConcurrent=%d", p.MaxConcurrent)
	}
	if p.MaxSubtasks <= 0 {
		t.Error("sub-task count must be bounded")
	}
	if err := p.Validate(); err != nil {
		t.Errorf("the sequential policy must be valid: %v", err)
	}

	// Parallel delegation is rejected — a small model + shared RAM can't afford it.
	if err := (SubagentPolicy{MaxConcurrent: 2, MaxSubtasks: 4}).Validate(); err == nil {
		t.Error("parallel sub-agents must be rejected")
	} else if !strings.Contains(err.Error(), "LONGRUN-005") {
		t.Errorf("rejection should cite the requirement: %v", err)
	}
	// A zero/negative sub-task bound is rejected.
	if err := (SubagentPolicy{MaxConcurrent: 1, MaxSubtasks: 0}).Validate(); err == nil {
		t.Error("an unbounded sub-task count must be rejected")
	}
}
