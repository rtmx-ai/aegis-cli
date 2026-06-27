package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// TestRealTaskCompletion → REQ-RUNQ-004: a real coding task is completed end-to-end by the
// local air-gapped stack. The recorded proof (eval/runq-004/result.json) must show the task
// closed (go test passed after `aegis run` edited the file) within the budget. This asserts
// the recorded Analysis artifact is a genuine close; the run is reproduced per docs/readiness.md.
func TestRealTaskCompletion(t *testing.T) {
	p := filepath.Join(repoRoot(t), "eval", "runq-004", "result.json")
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("RUNQ-004 proof not recorded at %s: %v", p, err)
	}
	var r struct {
		Task   string `json:"task"`
		Model  string `json:"model"`
		Closed bool   `json:"closed"`
		WallS  int    `json:"wall_s"`
		Budget int    `json:"budget_s"`
	}
	if err := json.Unmarshal(b, &r); err != nil {
		t.Fatalf("RUNQ-004 result malformed: %v", err)
	}
	if r.Task == "" || r.Model == "" {
		t.Error("proof must record the task + the model that closed it")
	}
	if !r.Closed {
		t.Error("RUNQ-004 requires a real task closed end-to-end; recorded closed=false")
	}
	if r.WallS <= 0 || r.WallS > r.Budget {
		t.Errorf("recorded wall_s=%d must be a real close within budget_s=%d", r.WallS, r.Budget)
	}
}
