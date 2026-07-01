package loop

import (
	"context"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// fbHarness records the feedback string each Drive received.
type fbHarness struct {
	*harness.Fake
	feedbacks []string
}

func (h *fbHarness) Drive(ctx context.Context, req *rtmx.Requirement, feedback string) (harness.Diff, error) {
	h.feedbacks = append(h.feedbacks, feedback)
	return h.Fake.Drive(ctx, req, feedback)
}

// failThenPass verifies false once (with output) then true — modeling a fix.
type failThenPass struct {
	*rtmx.Fake
	calls int
}

func (r *failThenPass) Verify(ctx context.Context, id string) (bool, string, error) {
	r.calls++
	if r.calls == 1 {
		return false, "TESTFAIL: boom at foo.go:10", nil
	}
	return true, "", nil
}

// TestInnerVerifyLoop → REQ-LONGRUN-001: after a failed verify the loop feeds the
// test output into the next drive (run -> inspect -> fix) and closes on the
// passing verify. The second drive must carry the first failure's output verbatim.
func TestInnerVerifyLoop(t *testing.T) {
	rt := &failThenPass{Fake: rtmxWithPassing("A-001")}
	h := &fbHarness{Fake: harnessFake()}
	l, _, _ := newLoop(testCfg(), rt, h) // Retries=1 -> 2 attempts

	res, err := l.Run(ctx(), true)
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if res.Closed != 1 || res.Parked != 0 {
		t.Fatalf("fail-then-pass: want closed=1 parked=0, got %+v", res)
	}
	if len(h.feedbacks) != 2 {
		t.Fatalf("want 2 drives (fail then fix), got %d: %v", len(h.feedbacks), h.feedbacks)
	}
	if h.feedbacks[0] != "" {
		t.Errorf("first drive feedback must be empty, got %q", h.feedbacks[0])
	}
	if !strings.Contains(h.feedbacks[1], "TESTFAIL: boom") {
		t.Errorf("second drive must receive the failed verify output as feedback, got %q", h.feedbacks[1])
	}
}

// TestFeedbackVerbatim → REQ-THINK-003: the loop feeds the verify/test output
// into the next drive VERBATIM (byte-identical) — the model sees the real failure
// text, not a paraphrase.
func TestFeedbackVerbatim(t *testing.T) {
	const exact = "--- FAIL: TestX\n    x.go:9: got 3 want 4\n"
	rt := rtmxWithFailing("A-001")
	rt.VerifyOutput["A-001"] = exact
	h := &fbHarness{Fake: harnessFake()}
	l, _, _ := newLoop(testCfg(), rt, h)
	if _, err := l.Run(ctx(), true); err != nil {
		t.Fatalf("run: %v", err)
	}
	if len(h.feedbacks) < 2 {
		t.Fatalf("want >=2 drives, got %d", len(h.feedbacks))
	}
	if h.feedbacks[1] != exact {
		t.Errorf("feedback must be verbatim: got %q, want %q", h.feedbacks[1], exact)
	}
}
