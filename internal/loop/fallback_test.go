package loop

import (
	"strings"
	"testing"
)

// TestModelFallback → REQ-LONGRUN-010: the fallback policy triggers after M
// consecutive identical failures, and the loop injects the fallback directive into
// a later drive so the agent varies its approach before parking.
func TestModelFallback(t *testing.T) {
	// Pure policy.
	p := FallbackPolicy{AfterFailures: 2, Temperature: 1.0}
	if do, _ := p.Fallback(1); do {
		t.Error("no fallback before M identical failures")
	}
	if do, temp := p.Fallback(2); !do || temp != 1.0 {
		t.Errorf("fallback at M: do=%v temp=%v, want true/1.0", do, temp)
	}
	if do, _ := (FallbackPolicy{}).Fallback(9); do {
		t.Error("zero-value policy must be disabled")
	}

	// Loop: identical verify failures -> the fallback directive is injected.
	cfg := testCfg()
	cfg.Retries = 3 // 4 attempts
	rt := rtmxWithFailing("A-001")
	rt.VerifyOutput["A-001"] = "same-fail"
	h := &fbHarness{Fake: harnessFake()}
	l, err := New(cfg, Deps{RTMX: rt, Harness: h, Fallback: FallbackPolicy{AfterFailures: 2, Temperature: 1.0}})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := l.Run(ctx(), true); err != nil {
		t.Fatalf("run: %v", err)
	}
	injected := false
	for _, fb := range h.feedbacks {
		if strings.Contains(fb, "materially different") {
			injected = true
		}
	}
	if !injected {
		t.Errorf("fallback directive must be injected after M identical failures; got %v", h.feedbacks)
	}
}
