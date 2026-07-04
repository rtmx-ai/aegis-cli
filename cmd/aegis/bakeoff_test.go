package main

import (
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// TestBakeoffSuiteAndMeasurement → REQ-BENCH-010: the rig's measurement primitives are ground-truth, not
// interpretation — files-edited comes from git, closed comes from the verify command, and each default
// task genuinely fails before a run (so "closed" means something). This is what makes the bake-off
// objective: a model that writes nothing scores 0 edits; a model that writes the right code passes verify.
func TestBakeoffSuiteAndMeasurement(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git unavailable")
	}
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go unavailable")
	}
	suite := defaultSuite()
	if len(suite) < 3 {
		t.Fatalf("default suite must have >=3 tasks; got %d", len(suite))
	}

	for _, task := range suite {
		ws := t.TempDir()
		if err := seedTask(ws, task); err != nil {
			t.Fatalf("seed %s: %v", task.Name, err)
		}
		// Precondition: every task must FAIL before any edit (else "closed" is meaningless).
		if runVerify(ws, task.Verify) {
			t.Errorf("task %s passed before any edit — not a valid agentic task", task.Name)
		}
		// A clean seed (committed) shows zero edits.
		if n := gitEditedCount(ws); n != 0 {
			t.Errorf("task %s: clean seed must show 0 edits, got %d", task.Name, n)
		}
	}

	// End-to-end on go-add: applying the correct edit makes files-edited>0 AND verify pass.
	ws := t.TempDir()
	add := suite[0] // go-add
	if err := seedTask(ws, add); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ws, "add.go"), []byte("package task\n\nfunc Add(a, b int) int { return a + b }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := gitEditedCount(ws); got != 1 {
		t.Errorf("after editing add.go, files-edited = %d, want 1", got)
	}
	if !runVerify(ws, add.Verify) {
		t.Error("go-add must pass verify once Add returns a+b")
	}
}

// TestTranscriptTurnsTokens → REQ-BENCH-010: the rig reads turns + total tokens from the intent-bench
// transcript's final result line (the same format internal/bench writes), for the throughput/cost columns.
func TestTranscriptTurnsTokens(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "t.jsonl")
	body := `{"type":"assistant","message":{}}
{"type":"result","num_turns":4,"usage":{"input_tokens":1200,"output_tokens":350}}
`
	if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	turns, total, out := transcriptStats(p)
	if turns != 4 || total != 1550 || out != 350 {
		t.Errorf("turns=%d total=%d out=%d, want 4 / 1550 / 350 (out=decode only, for honest tok/s)", turns, total, out)
	}
	// A missing transcript is benign (0/0/0), not a crash.
	if turns, total, out := transcriptStats(filepath.Join(dir, "nope.jsonl")); turns != 0 || total != 0 || out != 0 {
		t.Errorf("missing transcript must yield 0/0/0, got %d/%d/%d", turns, total, out)
	}
}

// TestBakeoffModelSelection → REQ-BENCH-011: the auto-select UX turns a selection line into model ids —
// "all", 1-based indices, or bare ids — so a one-command bake-off can pick host-suitable models without
// the clunky ">=2 --models" requirement. (The serve-each orchestration it feeds is M5-validated.)
func TestBakeoffModelSelection(t *testing.T) {
	choices := []modelChoice{
		{ID: "gemma-4-26b-a4b"},
		{ID: "devstral-small-2507"},
		{ID: "phi-4-mini"},
	}
	cases := []struct {
		in   string
		want []string
	}{
		{"all", []string{"gemma-4-26b-a4b", "devstral-small-2507", "phi-4-mini"}},
		{"1,3", []string{"gemma-4-26b-a4b", "phi-4-mini"}},
		{"2 2 2", []string{"devstral-small-2507"}}, // dedup
		{"devstral-small-2507", []string{"devstral-small-2507"}},
		{"", nil},
		{"9,bogus", nil}, // out-of-range + unknown are ignored, not fatal
	}
	for _, tc := range cases {
		got := parseModelSelection(tc.in, choices)
		if len(got) != len(tc.want) {
			t.Errorf("parseModelSelection(%q) = %v, want %v", tc.in, got, tc.want)
			continue
		}
		for i := range got {
			if got[i] != tc.want[i] {
				t.Errorf("parseModelSelection(%q)[%d] = %q, want %q", tc.in, i, got[i], tc.want[i])
			}
		}
	}

	// Explicit --models bypasses auto-select (and never blocks on a TTY).
	if got, rc := resolveBakeoffModels("a,b", false, false, io.Discard); rc != 0 || len(got) != 2 {
		t.Errorf("explicit --models must pass through: got %v rc=%d", got, rc)
	}
	// --no-serve without --models is an error (it measures the live endpoint, nothing to auto-select).
	if _, rc := resolveBakeoffModels("", false, true, io.Discard); rc == 0 {
		t.Error("--no-serve without --models must error")
	}
}

// TestBakeoffServeHelpers → REQ-BENCH-012: the auto-serve correctness helpers — the endpoint port is
// parsed so the temp calibration serves on the right port, and the calibration is written to a THROWAWAY
// path (never the operator's ~/.config/aegis/calibration.json) with the model + resolved ctx. This is
// what lets a multi-model run swap the served model per candidate instead of measuring one model twice.
func TestBakeoffServeHelpers(t *testing.T) {
	if endpointPort("http://127.0.0.1:8080") != 8080 {
		t.Errorf("endpointPort must parse 8080")
	}
	if endpointPort("http://127.0.0.1:9191/v1") != 9191 {
		t.Errorf("endpointPort must parse 9191 with a path")
	}
	if endpointPort("not a url") != 0 {
		t.Errorf("endpointPort must be 0 for an unparseable endpoint")
	}

	dir := t.TempDir()
	gguf := "/models/Devstral-Small-2507-IQ4_XS.gguf"
	p, err := writeBakeoffCalibration(gguf, dir, 8091)
	if err != nil {
		t.Fatalf("writeBakeoffCalibration: %v", err)
	}
	if filepath.Dir(p) != dir {
		t.Errorf("calibration must be written to the throwaway dir %q, got %q", dir, p)
	}
	body := func() string { b, _ := os.ReadFile(p); return string(b) }()
	for _, want := range []string{gguf, `"port": 8091`} {
		if !strings.Contains(body, want) {
			t.Errorf("bakeoff calibration missing %q:\n%s", want, body)
		}
	}
	if strings.Contains(body, `"ctx_size": 0`) || !strings.Contains(body, `"ctx_size"`) {
		t.Errorf("bakeoff calibration must carry a resolved ctx_size:\n%s", body)
	}
}

// TestGitEditedCountIgnoresDotPaths → REQ-BENCH-010: files-edited counts real source changes only, not
// tool droppings — a .opencode/ dir the harness may create in the workdir must NOT inflate the agency
// metric (the v1.9.4 bug where phantom edits made a no-op model look like it wrote code).
func TestGitEditedCountIgnoresDotPaths(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git unavailable")
	}
	ws := t.TempDir()
	if err := seedTask(ws, defaultSuite()[0]); err != nil {
		t.Fatal(err)
	}
	// A harness dropping (dot-dir) + a real edit: only the real edit counts.
	if err := os.MkdirAll(filepath.Join(ws, ".opencode"), 0o755); err != nil {
		t.Fatal(err)
	}
	_ = os.WriteFile(filepath.Join(ws, ".opencode", "state.json"), []byte("{}"), 0o644)
	_ = os.WriteFile(filepath.Join(ws, "add.go"), []byte("package task\n\nfunc Add(a, b int) int { return a + b }\n"), 0o644)
	if got := gitEditedCount(ws); got != 1 {
		t.Errorf("files-edited must count only the real source change (1), not the .opencode dropping; got %d", got)
	}
}
