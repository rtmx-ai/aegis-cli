package propose

import (
	"context"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// TestGateProposedNotClaimable models PROPOSE-002: proposed children are never
// returned by rtmx.Next until a human promotes them out of the proposed state.
func TestGateProposedNotClaimable(t *testing.T) {
	p := New(DefaultBounds(), nil)
	prop, err := p.Propose(context.Background(), parent(), []string{"a", "b"})
	if err != nil {
		t.Fatal(err)
	}
	// Seed an rtmx backlog with only the proposed children.
	reqs := make([]*rtmx.Requirement, len(prop.Children))
	for i := range prop.Children {
		c := prop.Children[i]
		reqs[i] = &c
	}
	f := rtmx.NewFake(reqs...)
	got, err := f.Next(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if got != nil {
		t.Fatalf("proposed children must not be claimable, Next returned %+v", got)
	}
}
