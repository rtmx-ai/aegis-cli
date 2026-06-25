package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// stageFakeOpencode writes a fake opencode at the staged path under dir and
// chdirs there, so opencode.ResolveBinary finds it.
func stageFakeOpencode(t *testing.T, dir, script string) {
	t.Helper()
	t.Chdir(dir)
	staged := filepath.Join(dir, "deploy", "opencode", "bin", "opencode")
	if err := os.MkdirAll(filepath.Dir(staged), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(staged, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
}

// TestRunWritesTranscript → covers cmdRun's happy path: prompt -> Solve -> transcript.
func TestRunWritesTranscript(t *testing.T) {
	dir := t.TempDir()
	stageFakeOpencode(t, dir, "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"step_finish\",\"part\":{\"reason\":\"stop\",\"text\":\"ok\"}}'\n")
	cfg := filepath.Join(dir, "aegis.json")
	if err := os.WriteFile(cfg, []byte(`{"endpoint":"http://127.0.0.1:11434","model_id":"m","harness":"builtin","allow_egress":false,"target":"linux-cpu"}`), 0o644); err != nil {
		t.Fatal(err)
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
