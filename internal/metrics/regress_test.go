package metrics

import "testing"

// TestRegressACRBelowBaselineFails models METRIC-002: an ACR below the rolling
// baseline (minus the allowed delta) fails the run. The comparison logic lives
// here so the gate is testable without the golden set.
func TestRegressACRBelowBaselineFails(t *testing.T) {
	const baseline, delta = 0.80, 0.05
	r := sampleCollector().Report() // ACR ~0.667
	if pass := r.ACR >= baseline-delta; pass {
		t.Fatalf("ACR %.3f should fail the regression gate (floor %.3f)", r.ACR, baseline-delta)
	}
}

func TestRegressACRAtBaselinePasses(t *testing.T) {
	const baseline, delta = 0.60, 0.05
	r := sampleCollector().Report() // ACR ~0.667
	if pass := r.ACR >= baseline-delta; !pass {
		t.Fatalf("ACR %.3f should clear the regression floor %.3f", r.ACR, baseline-delta)
	}
}
