package serving

import "testing"

// TestCtxSizeTunable → REQ-PERF-003: the served context defaults to 32k and is operator-tunable via
// AEGIS_CTX_SIZE, with the calibrated value in between.
func TestCtxSizeTunable(t *testing.T) {
	if DefaultCtxSize != 32768 {
		t.Errorf("DefaultCtxSize = %d; want 32768 (PERF-003)", DefaultCtxSize)
	}
	cal := &Calibration{}
	t.Setenv("AEGIS_CTX_SIZE", "65536")
	if got := cal.CtxSizeOrDefault(); got != 65536 {
		t.Errorf("AEGIS_CTX_SIZE must override: got %d", got)
	}
	t.Setenv("AEGIS_CTX_SIZE", "garbage")
	if got := cal.CtxSizeOrDefault(); got != DefaultCtxSize {
		t.Errorf("invalid AEGIS_CTX_SIZE must fall to the default: got %d", got)
	}
	t.Setenv("AEGIS_CTX_SIZE", "")
	cal.CtxSize = 8192
	if got := cal.CtxSizeOrDefault(); got != 8192 {
		t.Errorf("a calibrated ctx must be honored: got %d", got)
	}
	cal.CtxSize = 0
	if got := cal.CtxSizeOrDefault(); got != DefaultCtxSize {
		t.Errorf("unset ctx must be the 32k default: got %d", got)
	}
}

// TestResolveCtxSizeSingleSource → REQ-PERF-009: ResolveCtxSize is the ONE decision every consumer routes
// through — env override, else the caller's hint (a catalog num_ctx), else DefaultCtxSize. A too-small
// hint below the 512 floor is ignored so a stale/garbage value can never pin a tiny window.
func TestResolveCtxSizeSingleSource(t *testing.T) {
	if got := ResolveCtxSize(0); got != DefaultCtxSize {
		t.Errorf("no hint → default: got %d want %d", got, DefaultCtxSize)
	}
	if got := ResolveCtxSize(20480); got != 20480 {
		t.Errorf("a valid hint must be honored: got %d", got)
	}
	if got := ResolveCtxSize(64); got != DefaultCtxSize {
		t.Errorf("a sub-floor hint must fall to the default (never a tiny window): got %d", got)
	}
	t.Setenv("AEGIS_CTX_SIZE", "49152")
	if got := ResolveCtxSize(20480); got != 49152 {
		t.Errorf("the operator env override must win over any hint: got %d", got)
	}
}
