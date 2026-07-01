package loop

import (
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// TestGroundedHandoff → REQ-LONGRUN-006: the handoff carries the requirement, the
// deduped files touched (edits/writes only), decisions, and the next step, and
// renders them into a compact block that survives compaction.
func TestGroundedHandoff(t *testing.T) {
	req := &rtmx.Requirement{ID: "REQ-A-001", Title: "do the thing"}
	trace := []Step{
		{Tool: "edit", Args: "foo.go"},
		{Tool: "bash", Args: "go test"}, // not a file edit
		{Tool: "write", Args: "foo_test.go"},
		{Tool: "edit", Args: "foo.go"}, // duplicate
	}
	h := BuildHandoff(req, trace, []string{"used approach X"}, "run go test and fix B")

	if h.RequirementID != "REQ-A-001" {
		t.Errorf("requirement id: got %q", h.RequirementID)
	}
	if len(h.FilesTouched) != 2 {
		t.Errorf("files touched must dedupe to 2 (edits/writes only), got %v", h.FilesTouched)
	}

	r := h.Render()
	for _, want := range []string{"REQ-A-001", "do the thing", "foo.go", "foo_test.go", "used approach X", "run go test and fix B"} {
		if !strings.Contains(r, want) {
			t.Errorf("handoff render missing %q:\n%s", want, r)
		}
	}
}
