package main

import (
	"bytes"
	"strings"
	"testing"
)

// TestSurfaceGrammar → REQ-SURFACE-001: the hybrid grammar — curated verbs +
// pass-through namespaces — is reflected in the usage surface.
func TestSurfaceGrammar(t *testing.T) {
	var b bytes.Buffer
	usage(&b)
	for _, want := range []string{"run <prompt>", "loop", "rtmx", "code", "model", "pass-through"} {
		if !strings.Contains(b.String(), want) {
			t.Errorf("usage must advertise %q", want)
		}
	}
}

// TestRunOneShotAndLoopDrain → REQ-SURFACE-002: `run` is the one-shot agent task
// (needs a prompt); the drain moved to `loop` (accepts --once).
func TestRunOneShotAndLoopDrain(t *testing.T) {
	var o, e bytes.Buffer
	if code := run([]string{"run"}, &o, &e); code != 2 {
		t.Errorf("`run` with no prompt must exit 2 (one-shot needs a prompt), got %d", code)
	}
	var o2, e2 bytes.Buffer
	code := run([]string{"loop", "--once", "--config", "/nonexistent/aegis.json"}, &o2, &e2)
	if code == 0 {
		t.Error("`loop --once` with a bad config should fail")
	}
	if strings.Contains(e2.String(), "flag provided but not defined") {
		t.Errorf("`loop` must accept the drain flags (--once); got: %s", e2.String())
	}
}

// TestPassthroughNamespaces → REQ-SURFACE-003: rtmx/model are pass-through
// namespaces (routed to the inner tool), not unknown commands.
func TestPassthroughNamespaces(t *testing.T) {
	for _, ns := range []string{"rtmx", "model"} {
		var o, e bytes.Buffer
		run([]string{ns, "--no-such-flag-xyz"}, &o, &e)
		if strings.Contains(e.String(), "unknown command") {
			t.Errorf("%q must be a pass-through namespace, not an unknown command", ns)
		}
	}
}
