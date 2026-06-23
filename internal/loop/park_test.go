package loop

import (
	"bytes"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// TestParkDoesNotBlock models LOOP-006: unattended escalation parks (marks
// blocked + logs + releases + continues), it never blocks waiting for a human.
// A failing requirement followed by a passing one must still close the second.
func TestParkOnEscalationThenContinue(t *testing.T) {
	cfg := testCfg()
	cfg.BreakAfter = 3 // don't trip on the single failure
	rt := rtmx.NewFake(
		&rtmx.Requirement{ID: "BAD-001", Status: rtmx.StatusOpen},
		&rtmx.Requirement{ID: "OK-002", Status: rtmx.StatusOpen},
	)
	rt.VerifyResult["OK-002"] = true // BAD-001 stays false

	var buf bytes.Buffer
	al := audit.New(&buf, "test")
	l, err := New(cfg, Deps{RTMX: rt, Harness: harness.NewFake(), Audit: al, Metrics: metrics.NewCollector()})
	if err != nil {
		t.Fatal(err)
	}

	res, err := l.Run(ctx(), false)
	if err != nil {
		t.Fatal(err)
	}
	if res.Parked != 1 || res.Closed != 1 {
		t.Fatalf("res = %+v, want parked=1 closed=1 (parked one, continued)", res)
	}
	// Audit shows an explicit park entry for the blocked requirement.
	if !strings.Contains(buf.String(), `"action":"park"`) {
		t.Error("park must be recorded in the audit trail")
	}
}
