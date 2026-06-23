package propose

import (
	"bytes"
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
)

// TestProvenanceMachineAuthoredRecorded models PROPOSE-004: every proposed
// child is tagged machine-authored in the audit trail.
func TestProvenanceMachineAuthoredRecorded(t *testing.T) {
	var buf bytes.Buffer
	p := New(DefaultBounds(), audit.New(&buf, "aegis-propose"))
	if _, err := p.Propose(context.Background(), parent(), []string{"a", "b"}); err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	if len(lines) != 2 {
		t.Fatalf("want 2 audit lines, got %d", len(lines))
	}
	for _, ln := range lines {
		var e audit.Entry
		if err := json.Unmarshal([]byte(ln), &e); err != nil {
			t.Fatal(err)
		}
		if !e.MachineAuthored {
			t.Errorf("proposed child %s must be machine-authored", e.RequirementID)
		}
		if e.Action != audit.ActionPropose {
			t.Errorf("action = %q, want propose", e.Action)
		}
	}
}
