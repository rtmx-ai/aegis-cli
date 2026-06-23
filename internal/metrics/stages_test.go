package metrics

import (
	"testing"
	"time"
)

// TestStagesTimingEmitted models METRIC-003: the per-stage breakdown
// (prefill/decode/verify/harness-overhead) is aggregated and emitted.
func TestStagesTimingEmitted(t *testing.T) {
	c := NewCollector()
	c.Record(Attempt{RequirementID: "A", Closed: true, Stages: Stages{
		Prefill: 100 * time.Millisecond, Decode: 200 * time.Millisecond,
		Verify: 50 * time.Millisecond, HarnessOverhead: 25 * time.Millisecond,
	}})
	c.Record(Attempt{RequirementID: "B", Closed: true, Stages: Stages{
		Prefill: 100 * time.Millisecond, Decode: 200 * time.Millisecond,
		Verify: 50 * time.Millisecond, HarnessOverhead: 25 * time.Millisecond,
	}})
	r := c.Report()
	if r.Stages.Prefill != 200*time.Millisecond {
		t.Errorf("prefill = %s, want 200ms", r.Stages.Prefill)
	}
	if r.Stages.Decode != 400*time.Millisecond {
		t.Errorf("decode = %s, want 400ms", r.Stages.Decode)
	}
	if r.Stages.Verify != 100*time.Millisecond {
		t.Errorf("verify = %s, want 100ms", r.Stages.Verify)
	}
	if r.Stages.HarnessOverhead != 50*time.Millisecond {
		t.Errorf("harness overhead = %s, want 50ms", r.Stages.HarnessOverhead)
	}
}
