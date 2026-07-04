package offline

import (
	"encoding/csv"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestIntentBenchAgentWiring → REQ-BENCH-014: the intent-bench integration (the aegis agent shim + the
// run-suite driver) is present and honors the harness contract, so it can't silently rot before the real
// corpus run (BENCH-009). The shim drives `aegis run` with --no-intent (intent-bench controls the A/B via
// the workdir's seeded .mcp.json, not aegis's own repo), producing the contracted transcript; the runner
// serves the model target-aware (darwin-metal + linux-cpu) on a port OUTSIDE bench.sh's {8080,3000,5000}
// reclaim list, and drives both conditions (control + rtmx treatment) via `bench.sh --agent aegis`.
func TestIntentBenchAgentWiring(t *testing.T) {
	agent := readRepoFile(t, "deploy/intent-bench/aegis.sh")
	for _, want := range []string{
		"<workdir> <model> <prompt_file> <result_dir> <max_budget>", // the intent-bench agent contract
		"aegis", "run", "--no-intent", // drives aegis; A/B is workdir-controlled
		"--prompt-file", "--out", // reads the prompt file, writes the transcript
		"transcript.jsonl", "stderr.log", // the two contracted artifacts
		"AEGIS_ENDPOINT", // loopback endpoint (air-gap; no cloud model)
	} {
		if !strings.Contains(agent, want) {
			t.Errorf("deploy/intent-bench/aegis.sh missing %q — the agent contract/wiring is incomplete", want)
		}
	}
	if strings.Contains(agent, "anthropic") || strings.Contains(agent, "api.openai") {
		t.Error("the aegis agent must drive a LOCAL loopback model, never a cloud endpoint")
	}

	runner := readRepoFile(t, "deploy/intent-bench/run-suite.sh")
	for _, want := range []string{
		"aegis", "serve", // brings the model up via the production serve path
		"darwin-metal", "linux-cpu", // target-aware so it runs on the M5, not just linux
		"8090",                                                 // a serving port OUTSIDE bench.sh's {8080,3000,5000} reclaim list
		"--agent aegis",                                        // plugs aegis into intent-bench
		"--condition control", "treatment", "--treatment rtmx", // both A/B arms
	} {
		if !strings.Contains(runner, want) {
			t.Errorf("deploy/intent-bench/run-suite.sh missing %q — the BENCH-009 run wiring is incomplete", want)
		}
	}
	// It must NOT serve on a port bench.sh reclaims between runs (that was the swap/collision failure mode).
	if strings.Contains(runner, "--port 8080") || strings.Contains(runner, "PORT=8080") {
		t.Error("run-suite must not serve on :8080 — bench.sh reclaims it between runs, killing the model server")
	}
}

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
