package loop

import (
	"testing"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// TestBudgetCapsRequirements models LOOP-008 (requirement cap).
func TestBudgetCapsRequirements(t *testing.T) {
	cfg := testCfg()
	cfg.Budget.MaxRequirements = 2
	rt := rtmxWithPassing("A-001", "A-002", "A-003", "A-004")
	l, _, _ := newLoop(cfg, rt, harnessFake())

	res, err := l.Run(ctx(), false)
	if err != nil {
		t.Fatal(err)
	}
	if !res.BudgetExhausted || res.Attempted != 2 {
		t.Fatalf("res = %+v, want budget-exhausted at 2", res)
	}
}

// TestBudgetCapsWallClock models LOOP-008 (wall-clock cap) using an injected clock.
func TestBudgetCapsWallClock(t *testing.T) {
	cfg := config.Default()
	cfg.Retries = 0
	cfg.BreakAfter = 99
	cfg.Budget = config.Budget{MaxRequirements: 0, WallClock: 5 * time.Second}

	rt := rtmxWithPassing("A-001", "A-002", "A-003", "A-004", "A-005", "A-006")
	mc := metrics.NewCollector()
	al := audit.New(discard{}, "test")
	// Clock advances 2s per call; the loop checks the clock at the top of each
	// iteration, so the budget trips once elapsed >= 5s.
	l, err := New(cfg, Deps{
		RTMX: rt, Harness: harness.NewFake(), Audit: al, Metrics: mc,
		Now: fixedClock(time.Unix(0, 0), 2*time.Second),
	})
	if err != nil {
		t.Fatal(err)
	}
	res, err := l.Run(ctx(), false)
	if err != nil {
		t.Fatal(err)
	}
	if !res.BudgetExhausted {
		t.Fatalf("wall-clock budget should halt the run, res = %+v", res)
	}
	_ = rtmx.StatusOpen
}

type discard struct{}

func (discard) Write(p []byte) (int, error) { return len(p), nil }
