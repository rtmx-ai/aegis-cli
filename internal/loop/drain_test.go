package loop

import "testing"

// TestDrainUntilEmpty models LOOP-005: `aegis run` (no --once) drains the
// backlog until it is empty.
func TestDrainUntilEmpty(t *testing.T) {
	rt := rtmxWithPassing("A-001", "A-002", "A-003")
	l, _, _ := newLoop(testCfg(), rt, harnessFake())

	res, err := l.Run(ctx(), false)
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if res.Attempted != 3 || res.Closed != 3 {
		t.Fatalf("res = %+v, want 3 attempted/closed", res)
	}
	if res.BreakerTripped || res.BudgetExhausted {
		t.Fatal("clean drain should not trip breaker or budget")
	}
}
