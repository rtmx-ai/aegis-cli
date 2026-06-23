package loop

import (
	"context"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// TestResumeClaimReleasedOnEveryPath models LOOP-003: the loop is resumable
// after interruption because a claim never survives a completed iteration —
// closed and parked both release, leaving the requirement re-claimable.
func TestResumeClaimReleasedAfterClose(t *testing.T) {
	rt := rtmxWithPassing("A-001")
	l, _, _ := newLoop(testCfg(), rt, harnessFake())
	if _, err := l.Run(ctx(), true); err != nil {
		t.Fatal(err)
	}
	// After closing, the claim is released: a fresh claim must succeed.
	if err := rt.Claim(context.Background(), "A-001"); err != nil {
		t.Fatalf("claim should be free after close: %v", err)
	}
}

func TestResumeClaimReleasedAfterPark(t *testing.T) {
	rt := rtmxWithFailing("A-001")
	l, _, _ := newLoop(testCfg(), rt, harnessFake())
	if _, err := l.Run(ctx(), true); err != nil {
		t.Fatal(err)
	}
	if err := rt.Claim(context.Background(), "A-001"); err != nil {
		t.Fatalf("claim should be free after park: %v", err)
	}
	_ = rtmx.StatusBlocked
}
