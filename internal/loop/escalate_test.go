package loop

import (
	"context"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// TestEscalateAfterNRetries models LOOP-002: a failing verify retries up to N
// then escalates (parked), and the harness is driven N+1 times.
func TestEscalateAfterNRetries(t *testing.T) {
	cfg := testCfg() // Retries=1 -> 2 attempts
	rt := rtmxWithFailing("A-001")
	h := harnessFake()
	l, mc, _ := newLoop(cfg, rt, h)

	res, err := l.Run(ctx(), true)
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if res.Parked != 1 || res.Closed != 0 {
		t.Fatalf("res = %+v, want parked=1 closed=0", res)
	}
	if h.Calls != cfg.Retries+1 {
		t.Errorf("harness driven %d times, want %d (N+1)", h.Calls, cfg.Retries+1)
	}
	// Requirement is left blocked, not claimed.
	if err := rt.WriteStatus(context.Background(), "A-001", rtmx.StatusBlocked); err != nil {
		t.Fatal(err)
	}
	r := mc.Report()
	if r.Escalated != 1 {
		t.Errorf("metrics escalated = %d, want 1", r.Escalated)
	}
}
