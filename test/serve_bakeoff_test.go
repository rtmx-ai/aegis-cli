package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// TestBakeoffRecorded → REQ-SERVE-016: a documented model bake-off over >=3 candidate
// local models is recorded (eval/bakeoff/results.json), each scored on completion (the
// north star), WCR (latency), and TCR (tokens), with a winner selected from the field.
// The bake-off itself is run by scripts/serve-bakeoff.py against the local models; this
// is the Analysis artifact — it asserts the recorded result is well-formed so the
// decision is traceable and re-runnable.
func TestBakeoffRecorded(t *testing.T) {
	p := filepath.Join(repoRoot(t), "eval", "bakeoff", "results.json")
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("bake-off result not recorded at %s: %v (run scripts/serve-bakeoff.py)", p, err)
	}
	var r struct {
		Candidates []struct {
			Model      string  `json:"model"`
			Attempted  int     `json:"attempted"`
			Completion float64 `json:"completion"`
			WCRMs      int     `json:"wcr_ms"`
			TCR        int     `json:"tcr"`
		} `json:"candidates"`
		Winner string   `json:"winner"`
		Tasks  []string `json:"tasks"`
	}
	if err := json.Unmarshal(b, &r); err != nil {
		t.Fatalf("bake-off results.json malformed: %v", err)
	}
	if len(r.Candidates) < 3 {
		t.Fatalf("bake-off must score >=3 candidates, got %d", len(r.Candidates))
	}
	if len(r.Tasks) == 0 {
		t.Error("bake-off must record the task set it scored on")
	}
	for _, c := range r.Candidates {
		// completion/WCR/TCR may legitimately be zero (a weak model), but every
		// candidate must have actually been attempted and named.
		if c.Model == "" || c.Attempted == 0 {
			t.Errorf("candidate not actually scored: %+v", c)
		}
	}
	if r.Winner == "" {
		t.Fatal("bake-off must record a winner")
	}
	found := false
	for _, c := range r.Candidates {
		if c.Model == r.Winner {
			found = true
		}
	}
	if !found {
		t.Errorf("recorded winner %q is not among the scored candidates", r.Winner)
	}
}
