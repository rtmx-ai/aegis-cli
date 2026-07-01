package loop

import (
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/harness"
)

// TestLoopParksStuckAgent covers the live wiring of LONGRUN-009 over the harness
// step-stream (Diff.Trace): a harness that returns a looping trajectory is parked
// after ONE drive — before it verifies or burns the remaining retries — and
// flagged as stuck, not merely retry-exhausted.
func TestLoopParksStuckAgent(t *testing.T) {
	rt := rtmxWithPassing("A-001") // verify WOULD pass, but a stuck agent never reaches it
	h := harnessFake()
	step := harness.Event{Tool: "read", Args: "a.go", Obs: "same"}
	h.Trace = []harness.Event{step, step, step, step} // 4 identical -> repeated-action

	l, _, _ := newLoop(testCfg(), rt, h)
	res, err := l.Run(ctx(), true)
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if res.Parked != 1 || res.Stuck != 1 || res.Closed != 0 {
		t.Errorf("stuck agent: want parked=1 stuck=1 closed=0, got %+v", res)
	}
	if h.Calls != 1 {
		t.Errorf("stuck agent must short-circuit the retry loop: drove %d times, want 1", h.Calls)
	}
}
