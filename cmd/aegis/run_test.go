package main

import (
	"bytes"
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

func openReqs(ids ...string) []*rtmx.Requirement {
	rs := make([]*rtmx.Requirement, len(ids))
	for i, id := range ids {
		rs[i] = &rtmx.Requirement{ID: id, Status: rtmx.StatusOpen}
	}
	return rs
}

func fakeWithVerify(verify bool, ids ...string) *rtmx.Fake {
	f := rtmx.NewFake(openReqs(ids...)...)
	for _, id := range ids {
		f.VerifyResult[id] = verify
	}
	return f
}

// TestRunLiveDrains → REQ-RUN-001: liveRun drives the loop to drain the backlog.
func TestRunLiveDrains(t *testing.T) {
	fake := fakeWithVerify(true, "REQ-A-1", "REQ-A-2")
	var out bytes.Buffer
	res, err := liveRun(context.Background(), config.Default(), runDeps{
		RTMX: fake, Harness: harness.NewFake(), Audit: audit.New(&bytes.Buffer{}, "t"),
	}, false, &out)
	if err != nil {
		t.Fatalf("liveRun: %v", err)
	}
	if res.Closed != 2 {
		t.Errorf("expected 2 closed on drain, got %d", res.Closed)
	}
}

// TestRunPreflightAbortsWhenUnhealthy → REQ-RUN-002: a failing serving preflight
// aborts the run before any work is claimed.
func TestRunPreflightAbortsWhenUnhealthy(t *testing.T) {
	fake := fakeWithVerify(true, "REQ-A-1")
	var out bytes.Buffer
	res, err := liveRun(context.Background(), config.Default(), runDeps{
		RTMX: fake, Harness: harness.NewFake(), Audit: audit.New(&bytes.Buffer{}, "t"),
		Preflight: func(context.Context) error { return errors.New("endpoint down") },
	}, true, &out)
	if err == nil || !strings.Contains(err.Error(), "preflight") {
		t.Fatalf("expected a preflight error, got %v", err)
	}
	if res.Attempted != 0 {
		t.Errorf("no work must be attempted when preflight fails, got attempted=%d", res.Attempted)
	}
}

// TestRunWritesAuditAndSummary → REQ-RUN-003: the run writes claim/verify audit
// lines and prints a summary.
func TestRunWritesAuditAndSummary(t *testing.T) {
	fake := fakeWithVerify(true, "REQ-A-1")
	var auditBuf, out bytes.Buffer
	if _, err := liveRun(context.Background(), config.Default(), runDeps{
		RTMX: fake, Harness: harness.NewFake(), Audit: audit.New(&auditBuf, "aegis-loop"),
	}, true, &out); err != nil {
		t.Fatal(err)
	}
	a := auditBuf.String()
	if !strings.Contains(a, `"action":"claim"`) || !strings.Contains(a, `"action":"verify"`) {
		t.Errorf("audit must record claim + verify:\n%s", a)
	}
	if !strings.Contains(out.String(), "closed=1") {
		t.Errorf("summary must report closed=1:\n%s", out.String())
	}
}

// TestRunHonorsBudgetAndBreaker → REQ-RUN-005: budget and circuit breaker stop
// the live run.
func TestRunHonorsBudgetAndBreaker(t *testing.T) {
	// Breaker: repeated verify failures trip after BreakAfter parks.
	cfg := config.Default()
	cfg.BreakAfter = 2
	breaker := fakeWithVerify(false, "REQ-A-1", "REQ-A-2", "REQ-A-3", "REQ-A-4", "REQ-A-5")
	res, err := liveRun(context.Background(), cfg, runDeps{
		RTMX: breaker, Harness: harness.NewFake(), Audit: audit.New(&bytes.Buffer{}, "t"),
	}, false, &bytes.Buffer{})
	if err != nil {
		t.Fatal(err)
	}
	if !res.BreakerTripped {
		t.Errorf("circuit breaker must trip on repeated failures, got %+v", res)
	}

	// Budget: cap the session at one requirement.
	cfg2 := config.Default()
	cfg2.Budget.MaxRequirements = 1
	budget := fakeWithVerify(true, "REQ-B-1", "REQ-B-2", "REQ-B-3")
	res2, err := liveRun(context.Background(), cfg2, runDeps{
		RTMX: budget, Harness: harness.NewFake(), Audit: audit.New(&bytes.Buffer{}, "t"),
	}, false, &bytes.Buffer{})
	if err != nil {
		t.Fatal(err)
	}
	if !res2.BudgetExhausted || res2.Closed != 1 {
		t.Errorf("budget must cap the run at 1, got %+v", res2)
	}
}

// TestRunRefusesOpenEnv → REQ-RUN-004: `aegis run` refuses to start when egress
// is enabled.
func TestRunRefusesOpenEnv(t *testing.T) {
	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "aegis.json")
	cfg := `{"endpoint":"http://127.0.0.1:8080","harness":"builtin","target":"linux-cpu",` +
		`"retries":2,"break_after":3,"budget":{"max_requirements":0,"wall_clock":0},` +
		`"audit_path":"audit/log.jsonl","calibration_path":"deploy/llama-server/calibration.json","allow_egress":true}`
	if err := os.WriteFile(cfgPath, []byte(cfg), 0o644); err != nil {
		t.Fatal(err)
	}
	var out, errBuf bytes.Buffer
	code := run([]string{"loop", "--once", "--config", cfgPath}, &out, &errBuf)
	if code == 0 {
		t.Error("loop must refuse an egress-enabled config")
	}
	if !strings.Contains(errBuf.String(), "egress") {
		t.Errorf("error must mention egress, got: %s", errBuf.String())
	}
}

// TestFrameReports → REQ-FRAME-004: frameReport classifies the backlog and
// surfaces the reframe (parked) and unframed lists.
func TestFrameReports(t *testing.T) {
	reqs := []*rtmx.Requirement{
		{ID: "REQ-A-1", Status: rtmx.StatusClosed, SpecFile: "s.md"},
		{ID: "REQ-A-2", Status: rtmx.StatusBlocked, SpecFile: "s.md"},
		{ID: "REQ-OLD", Status: rtmx.StatusOpen}, // unframed
	}
	var out bytes.Buffer
	frameReport(reqs, &out)
	s := out.String()
	for _, want := range []string{"delivered=1", "parked=1", "REQ-A-2", "unframed", "REQ-OLD"} {
		if !strings.Contains(s, want) {
			t.Errorf("frame report missing %q:\n%s", want, s)
		}
	}
}
