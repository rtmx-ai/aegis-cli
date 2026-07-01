package opencode

import (
	"strings"
	"testing"
)

// TestPlanThenAct → REQ-LONGRUN-004: planning is gated by complexity — trivial tasks
// skip the Plan phase, non-trivial (multi-week or compound) tasks plan before building.
func TestPlanThenAct(t *testing.T) {
	if PlanFirst(0.2, "fix a typo") {
		t.Error("a trivial task should skip the plan phase")
	}
	if !PlanFirst(2.0, "refactor the control loop") {
		t.Error("a multi-week task should plan first")
	}
	if !PlanFirst(0.3, "add caching and update the docs and wire metrics") {
		t.Error("a compound multi-step task should plan first")
	}
	if !PlanFirst(0.3, "migrate config; then rewire launch") {
		t.Error("a semicolon-joined multi-step task should plan first")
	}
	// The plan directive is concrete guidance, not filler.
	if !strings.Contains(planDirective, "plan") {
		t.Error("plan directive should instruct planning")
	}
}
