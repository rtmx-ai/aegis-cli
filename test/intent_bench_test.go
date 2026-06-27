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
	// BENCH-009 means the REAL intent-bench corpus — the multi-requirement project
	// experiments (url-shortener: 10 reqs, task-manager: 13 reqs) with rtmx as the
	// treatment (treatments/rtmx.sh) vs control. A toy single-function suite does NOT
	// satisfy it. Skip (keeping the requirement MISSING) until the real corpus is run;
	// scripts/intent-bench.py with the toy EXPERIMENTS is a methodology demo, not this.
	for _, exp := range []string{"url-shortener", "task-manager"} {
		if !seen[exp]["control"] || !seen[exp]["treatment"] {
			t.Skipf("intent-bench corpus not run (missing %q for control+treatment) — the current "+
				"eval/intent-bench data is a methodology demo only; BENCH-009 needs the real "+
				"intent-bench experiments (url-shortener, task-manager) as the rtmx treatment", exp)
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
