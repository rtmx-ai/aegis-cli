package opencode

import (
	"bytes"
	"strings"
	"testing"
	"time"
)

// TestLaunchSplash → OC-048: bare `aegis` must show an immediate "Loading aegis…" splash instead of a
// blank terminal during the model load. The splash writes its banner synchronously (instant feedback)
// and returns an idempotent, non-blocking stop func. On a non-TTY writer (a pipe/test buffer) it is a
// one-shot banner with a no-op stop — proven here by the fast return and repeated stop calls.
func TestLaunchSplash(t *testing.T) {
	var buf bytes.Buffer
	stop := LaunchSplash(&buf)
	if stop == nil {
		t.Fatal("LaunchSplash must return a non-nil stop func")
	}
	out := buf.String()
	if !strings.Contains(out, "Loading aegis") {
		t.Errorf("splash must print the immediate \"Loading aegis…\" banner; got:\n%q", out)
	}
	if !strings.Contains(out, "aegis — air-gapped agentic coding") {
		t.Errorf("splash must identify aegis; got:\n%q", out)
	}
	// The stop func must be idempotent and return promptly on a non-TTY (no spinner goroutine to join).
	done := make(chan struct{})
	go func() { stop(); stop(); close(done) }()
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("stop() must return promptly and be safe to call twice")
	}
}
