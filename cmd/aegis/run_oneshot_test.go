package main

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/opencode"
)

// TestRunWritesTranscript → covers cmdRun's happy path: prompt -> serve drive ->
// transcript. The serve drive is injected here (the real one is covered by the
// gated internal/opencode serve-drive integration test); this asserts the
// command's transcript-writing path.
func TestRunWritesTranscript(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)
	cfg := filepath.Join(dir, "aegis.json")
	if err := os.WriteFile(cfg, []byte(`{"endpoint":"http://127.0.0.1:11434","model_id":"m","harness":"builtin","allow_egress":false,"target":"linux-cpu"}`), 0o644); err != nil {
		t.Fatal(err)
	}

	orig := runSolve
	t.Cleanup(func() { runSolve = orig })
	runSolve = func(_ context.Context, _ config.Config, _ string, opts opencode.SolveOptions) (*opencode.SolveResult, error) {
		if opts.Prompt != "x" {
			t.Errorf("cmdRun passed wrong prompt: %q", opts.Prompt)
		}
		return &opencode.SolveResult{Messages: []opencode.TranscriptMessage{
			{Role: "assistant", Tokens: opencode.Tokens{Total: 5}, Text: "ok"},
		}}, nil
	}

	outp := filepath.Join(dir, "t.jsonl")
	var o, e bytes.Buffer
	code := run([]string{"run", "--workdir", dir, "--config", cfg, "--model", "m", "--prompt", "x", "--out", outp}, &o, &e)
	if code != 0 {
		t.Fatalf("run exited %d: %s", code, e.String())
	}
	b, _ := os.ReadFile(outp)
	if !strings.Contains(string(b), `"result"`) {
		t.Errorf("transcript missing result record: %s", b)
	}
}

// TestTUIMissingBinary → cmdTUI prints guidance + exits 1 when opencode is absent.
func TestTUIMissingBinary(t *testing.T) {
	t.Chdir(t.TempDir()) // no staged opencode here
	var o, e bytes.Buffer
	if code := run([]string{"tui"}, &o, &e); code != 1 {
		t.Errorf("tui without opencode should exit 1, got %d", code)
	}
	if !strings.Contains(e.String(), "OpenCode") {
		t.Errorf("expected missing-OpenCode guidance, got: %s", e.String())
	}
}
