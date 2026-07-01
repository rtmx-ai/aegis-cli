package loop

import "testing"

// TestVerifierIsTest → REQ-THINK-002: the loop closes a requirement ONLY on a
// passing rtmx.Verify — never because the harness/model drove "successfully"
// (claimed done). A small local model is a weak self-judge, so the test suite is
// the sole close gate. The two cases below drive an identical successful harness;
// the only thing that changes is the verify result, proving it is the decider.
func TestVerifierIsTest(t *testing.T) {
	// Case A: harness drives successfully (the model "did the work") but verify
	// FAILS -> the requirement must NOT close; it parks.
	{
		rt := rtmxWithFailing("A-001") // Verify -> false
		h := harnessFake()             // Drive succeeds (no DriveErr)
		l, _, _ := newLoop(testCfg(), rt, h)
		res, err := l.Run(ctx(), true)
		if err != nil {
			t.Fatalf("run: %v", err)
		}
		if h.Calls == 0 {
			t.Fatal("harness never driven; guardrail test would be vacuous")
		}
		if res.Closed != 0 {
			t.Errorf("verify failed yet %d closed — the model, not the test, decided done", res.Closed)
		}
		if res.Parked != 1 {
			t.Errorf("verify failed: want parked=1, got %+v", res)
		}
	}
	// Case B: identical successful harness, verify PASSES -> closes. The verify
	// result is the only difference from Case A, so it is the deciding gate.
	{
		rt := rtmxWithPassing("A-001") // Verify -> true
		h := harnessFake()
		l, _, _ := newLoop(testCfg(), rt, h)
		res, err := l.Run(ctx(), true)
		if err != nil {
			t.Fatalf("run: %v", err)
		}
		if res.Closed != 1 || res.Parked != 0 {
			t.Errorf("verify passed: want closed=1 parked=0, got %+v", res)
		}
	}
}
