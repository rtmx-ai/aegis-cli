package offline

import (
	"encoding/csv"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// TestIntentBenchSuiteRun → REQ-BENCH-009 (+P01/P02): the full intent-bench suite ran over
// EVERY experiment for the aegis control + treatment conditions, recording a populated
// eval/intent-bench/summary.csv + comparison.json (per-condition completion). The cloud
// (claude-code) baseline is an out-of-enclave egress condition — recorded when available,
// not required for this air-gapped assertion.
func TestIntentBenchSuiteRun(t *testing.T) {
	root := repoRoot(t)
	f, err := os.Open(filepath.Join(root, "eval", "intent-bench", "summary.csv"))
	if err != nil {
		t.Fatalf("intent-bench summary not recorded: %v (run scripts/intent-bench.py)", err)
	}
	defer f.Close()
	rows, err := csv.NewReader(f).ReadAll()
	if err != nil || len(rows) < 2 {
		t.Fatalf("summary.csv has no data rows: %v", err)
	}
	seen := map[string]map[string]bool{}
	for _, r := range rows[1:] {
		if len(r) < 2 {
			continue
		}
		if seen[r[0]] == nil {
			seen[r[0]] = map[string]bool{}
		}
		seen[r[0]][r[1]] = true
	}
	// The suite must cover EVERY experiment for control + treatment (not just one).
	for _, exp := range []string{"go-add", "go-max", "go-fib"} {
		for _, cond := range []string{"control", "treatment"} {
			if !seen[exp][cond] {
				t.Errorf("summary missing %s/%s — the suite must drive every experiment for control + treatment", exp, cond)
			}
		}
	}
	b, err := os.ReadFile(filepath.Join(root, "eval", "intent-bench", "comparison.json"))
	if err != nil {
		t.Fatalf("comparison.json missing: %v", err)
	}
	var comp struct {
		ByCondition map[string]struct {
			Attempted int `json:"attempted"`
		} `json:"by_condition"`
	}
	if err := json.Unmarshal(b, &comp); err != nil {
		t.Fatalf("comparison.json malformed: %v", err)
	}
	for _, cond := range []string{"control", "treatment"} {
		if comp.ByCondition[cond].Attempted == 0 {
			t.Errorf("comparison.json must record condition %q with attempts", cond)
		}
	}
}
