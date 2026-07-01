package loop

import "testing"

// TestStuckDetector → REQ-LONGRUN-009: the semantic stuck detector flags
// repetition, repeated errors, monologue, ping-pong, and runaway condensation
// on the tail of a trajectory, and leaves varied progress alone — model-free.
func TestStuckDetector(t *testing.T) {
	th := DefaultStuckThresholds()
	mk := func(tool, args, obs string, err bool) Step { return Step{Tool: tool, Args: args, Obs: obs, Err: err} }

	// Varied, healthy progress is not stuck.
	varied := []Step{mk("read", "a.go", "...", false), mk("edit", "a.go", "ok", false), mk("bash", "go test", "pass", false)}
	if r := DetectStuck(varied, th); r != NotStuck {
		t.Errorf("varied: got %q, want NotStuck", r)
	}

	// Repeated identical action->obs pairs (4) trips; 3 does not.
	rep := make([]Step, 0, 4)
	for i := 0; i < 4; i++ {
		rep = append(rep, mk("read", "a.go", "same", false))
	}
	if r := DetectStuck(rep, th); r != StuckRepeatedAction {
		t.Errorf("repeated-action: got %q, want %q", r, StuckRepeatedAction)
	}
	if r := DetectStuck(rep[:3], th); r != NotStuck {
		t.Errorf("3 identical must not trip repeated-action(4): got %q", r)
	}

	// Repeated identical erroring actions (3), even with differing error text.
	rerr := []Step{mk("bash", "go build", "err1", true), mk("bash", "go build", "err2", true), mk("bash", "go build", "err3", true)}
	if r := DetectStuck(rerr, th); r != StuckRepeatedError {
		t.Errorf("repeated-error: got %q, want %q", r, StuckRepeatedError)
	}

	// Monologue: 3 consecutive agent messages with no tool call.
	mono := []Step{mk("", "thinking", "", false), mk("", "still thinking", "", false), mk("", "hmm", "", false)}
	if r := DetectStuck(mono, th); r != StuckMonologue {
		t.Errorf("monologue: got %q, want %q", r, StuckMonologue)
	}

	// Ping-pong: A,B repeated 3× (6 steps) trips; 2× (4 steps) does not.
	pp := []Step{mk("read", "x", "1", false), mk("edit", "x", "2", false), mk("read", "x", "1", false), mk("edit", "x", "2", false), mk("read", "x", "1", false), mk("edit", "x", "2", false)}
	if r := DetectStuck(pp, th); r != StuckPingPong {
		t.Errorf("ping-pong: got %q, want %q", r, StuckPingPong)
	}
	if r := DetectStuck(pp[:4], th); r != NotStuck {
		t.Errorf("2 cycles must not trip ping-pong(3): got %q", r)
	}

	// Runaway condensation (10) trips.
	cond := make([]Step, 0, 10)
	for i := 0; i < 10; i++ {
		cond = append(cond, Step{Kind: "condense"})
	}
	if r := DetectStuck(cond, th); r != StuckCondensing {
		t.Errorf("condensation: got %q, want %q", r, StuckCondensing)
	}

	// A stuck tail after a healthy prefix still trips (tail-sensitive).
	tail := append([]Step{mk("read", "a", "x", false), mk("edit", "b", "y", false)}, rep...)
	if r := DetectStuck(tail, th); r != StuckRepeatedAction {
		t.Errorf("stuck tail after healthy prefix: got %q, want %q", r, StuckRepeatedAction)
	}
}
