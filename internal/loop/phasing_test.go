package loop

import (
	"context"
	"sync"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// phaseTracker records whether generation and verification ever overlap.
type phaseTracker struct {
	mu         sync.Mutex
	generating bool
	verifying  bool
	overlapped bool
}

func (p *phaseTracker) enterGen() { p.set(&p.generating, true) }
func (p *phaseTracker) leaveGen() { p.set(&p.generating, false) }
func (p *phaseTracker) enterVer() { p.set(&p.verifying, true) }
func (p *phaseTracker) leaveVer() { p.set(&p.verifying, false) }

func (p *phaseTracker) set(flag *bool, v bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	*flag = v
	if p.generating && p.verifying {
		p.overlapped = true
	}
}

type phaseHarness struct {
	*harness.Fake
	tr *phaseTracker
}

func (h *phaseHarness) Drive(ctx context.Context, req *rtmx.Requirement, feedback string) (harness.Diff, error) {
	h.tr.enterGen()
	defer h.tr.leaveGen()
	return h.Fake.Drive(ctx, req, feedback)
}

type phaseRTMX struct {
	*rtmx.Fake
	tr *phaseTracker
}

func (r *phaseRTMX) Verify(ctx context.Context, id string) (bool, string, error) {
	r.tr.enterVer()
	defer r.tr.leaveVer()
	return r.Fake.Verify(ctx, id)
}

// TestPhasingVerifyNotConcurrentWithGeneration models LOOP-004: verify must not
// run concurrently with generation.
func TestPhasingVerifyNotConcurrentWithGeneration(t *testing.T) {
	tr := &phaseTracker{}
	base := rtmxWithPassing("A-001")
	rt := &phaseRTMX{Fake: base, tr: tr}
	h := &phaseHarness{Fake: harness.NewFake(), tr: tr}

	l, err := New(testCfg(), Deps{RTMX: rt, Harness: h, Metrics: metrics.NewCollector()})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := l.Run(ctx(), true); err != nil {
		t.Fatal(err)
	}
	if tr.overlapped {
		t.Fatal("generation and verification phases overlapped")
	}
}
