package propose

import (
	"bytes"
	"context"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

func parent() rtmx.Requirement {
	return rtmx.Requirement{ID: "LOOP", Prefix: "LOOP", Title: "make the loop production-ready",
		Status: rtmx.StatusOpen, Tests: []string{"loop/e2e_test"}}
}

func TestProposeEmitsProposedChildren(t *testing.T) {
	var buf bytes.Buffer
	p := New(DefaultBounds(), audit.New(&buf, "aegis-propose"))
	prop, err := p.Propose(context.Background(), parent(),
		[]string{"drain", "park-on-escalation", "circuit breaker", "run budget"})
	if err != nil {
		t.Fatal(err)
	}
	if len(prop.Children) != 4 {
		t.Fatalf("want 4 children, got %d", len(prop.Children))
	}
	for _, c := range prop.Children {
		if c.Status != rtmx.StatusProposed {
			t.Errorf("child %s status = %q, want proposed", c.ID, c.Status)
		}
	}
}
