package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestFairModelComparison → REQ-SERVE-022: the model comparison records per-candidate
// tool-call parsing fidelity on a parsing-correct path (llama.cpp --jinja), so candidates
// are scored on capability — the confounded SERVE-016/019 qwen3-coder result is re-validated.
func TestFairModelComparison(t *testing.T) {
	p := filepath.Join(repoRoot(t), "eval", "bakeoff", "fidelity.json")
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("fair-comparison artifact not recorded at %s: %v", p, err)
	}
	var r struct {
		Candidates []struct {
			Model  string `json:"model"`
			Parsed bool   `json:"parsed"`
			Closed bool   `json:"closed"`
			WallS  int    `json:"wall_s"`
		} `json:"candidates"`
	}
	if err := json.Unmarshal(b, &r); err != nil {
		t.Fatalf("fidelity.json malformed: %v", err)
	}
	if len(r.Candidates) < 2 {
		t.Fatalf("fair comparison must score >=2 candidates, got %d", len(r.Candidates))
	}
	var sawQwen bool
	for _, c := range r.Candidates {
		if c.Model == "" {
			t.Error("candidate unnamed")
		}
		// The whole point of SERVE-022: parsing fidelity is recorded per candidate.
		if !c.Parsed {
			t.Errorf("candidate %q scored with parsing fidelity=false — not a fair comparison", c.Model)
		}
		if strings.Contains(strings.ToLower(c.Model), "qwen") {
			sawQwen = true
			if !c.Closed {
				t.Errorf("qwen3-coder must be re-validated on a parsing-correct path; recorded closed=false")
			}
		}
	}
	if !sawQwen {
		t.Error("the fair comparison must include qwen3-coder (the previously-confounded candidate)")
	}
}
