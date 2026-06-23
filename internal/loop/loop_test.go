package loop

import (
	"bytes"
	"context"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// testCfg returns a validated config tuned for fast deterministic tests.
func testCfg() config.Config {
	c := config.Default()
	c.Retries = 1
	c.BreakAfter = 2
	c.Budget = config.Budget{MaxRequirements: 0, WallClock: 0}
	return c
}

// newLoop builds a loop over the given fakes plus a discard audit log and a
// collector, returning all three for assertions.
func newLoop(cfg config.Config, rt rtmx.Client, h harness.Adapter) (*Loop, *metrics.Collector, *audit.Log) {
	mc := metrics.NewCollector()
	al := audit.New(&bytes.Buffer{}, "test")
	l, err := New(cfg, Deps{RTMX: rt, Harness: h, Audit: al, Metrics: mc})
	if err != nil {
		panic(err)
	}
	return l, mc, al
}

func ctx() context.Context { return context.Background() }

// harnessFake returns a fresh fake harness adapter.
func harnessFake() *harness.Fake { return harness.NewFake() }

// rtmxWithPassing builds a fake rtmx whose listed requirements all verify true.
func rtmxWithPassing(ids ...string) *rtmx.Fake {
	reqs := make([]*rtmx.Requirement, len(ids))
	for i, id := range ids {
		reqs[i] = &rtmx.Requirement{ID: id, Status: rtmx.StatusOpen}
	}
	f := rtmx.NewFake(reqs...)
	for _, id := range ids {
		f.VerifyResult[id] = true
	}
	return f
}

// rtmxWithFailing builds a fake rtmx whose listed requirements all verify false.
func rtmxWithFailing(ids ...string) *rtmx.Fake {
	reqs := make([]*rtmx.Requirement, len(ids))
	for i, id := range ids {
		reqs[i] = &rtmx.Requirement{ID: id, Status: rtmx.StatusOpen}
	}
	return rtmx.NewFake(reqs...) // VerifyResult unset -> false
}

// fixedClock returns a Now func advancing by step on each call.
func fixedClock(start time.Time, step time.Duration) func() time.Time {
	cur := start
	return func() time.Time {
		t := cur
		cur = cur.Add(step)
		return t
	}
}
