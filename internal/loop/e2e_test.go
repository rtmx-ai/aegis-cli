package loop

import "testing"

// TestE2EClosesTrivialRequirement models LOOP-001: the full loop closes a
// trivial requirement end-to-end.
func TestE2EClosesTrivialRequirement(t *testing.T) {
	rt := rtmxWithPassing("A-001")
	h := harnessFake()
	l, mc, _ := newLoop(testCfg(), rt, h)

	res, err := l.Run(ctx(), true)
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if res.Closed != 1 || res.Attempted != 1 {
		t.Fatalf("res = %+v, want 1 attempted/closed", res)
	}
	r := mc.Report()
	if r.ACR != 1 {
		t.Errorf("ACR = %v, want 1.0", r.ACR)
	}
}
