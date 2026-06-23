package loop

import "testing"

// TestBreakerHaltsAfterMConsecutiveFailures models LOOP-007: the circuit
// breaker halts the run after M consecutive failures.
func TestBreakerHaltsAfterMConsecutiveFailures(t *testing.T) {
	cfg := testCfg()
	cfg.BreakAfter = 2
	// Five failing requirements; breaker should trip after the 2nd park.
	rt := rtmxWithFailing("F-001", "F-002", "F-003", "F-004", "F-005")
	l, _, _ := newLoop(cfg, rt, harnessFake())

	res, err := l.Run(ctx(), false)
	if err != nil {
		t.Fatal(err)
	}
	if !res.BreakerTripped {
		t.Fatal("breaker should trip on consecutive failures")
	}
	if res.Attempted != 2 {
		t.Fatalf("attempted = %d, want 2 (halt after M=2)", res.Attempted)
	}
}
