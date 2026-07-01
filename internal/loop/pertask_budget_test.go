package loop

import "testing"

// TestPerTaskBudget → REQ-LONGRUN-008: a task that consumes its per-task token
// budget is parked before it burns the remaining retries — an inner cap distinct
// from the retry count and the session-wide budget.
func TestPerTaskBudget(t *testing.T) {
	cfg := testCfg()              // Retries=1 -> 2 attempts
	cfg.Budget.PerTaskTokens = 50 // the fake harness returns 100 tokens per drive
	rt := rtmxWithFailing("A-001")
	h := harnessFake()
	l, _, _ := newLoop(cfg, rt, h)

	res, err := l.Run(ctx(), true)
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if res.Parked != 1 || res.Closed != 0 {
		t.Errorf("per-task budget: want parked=1 closed=0, got %+v", res)
	}
	if h.Calls != 1 {
		t.Errorf("per-task budget must short-circuit retries: drove %d times, want 1", h.Calls)
	}
}
