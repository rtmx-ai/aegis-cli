package main

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// TestServeHealsStaleCtx → REQ-PERF-009: a persisted calibration carrying a STALE, small ctx_size (an
// old 16384 that survived an aegis upgrade) must NOT pin the served window. buildServeCommand re-resolves
// through the one resolver every launch, so --ctx-size follows serving.DefaultCtxSize, not the stale value.
// This is the fix for the "30k requested, 16k available" field bug that persisted across an upgrade.
func TestServeHealsStaleCtx(t *testing.T) {
	t.Setenv("AEGIS_CTX_SIZE", "") // ensure no operator override skews the resolve
	dir := t.TempDir()
	cal := filepath.Join(dir, "cal.json")
	// Stale 16384 + a model path that matches no catalog entry, so the resolver falls to the default.
	if err := os.WriteFile(cal, []byte(`{"target":"linux-cpu","threads":8,"batch":256,"ngl":0,"model":"/m/stale-unknown.gguf","port":8080,"ctx_size":16384}`), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd, err := buildServeCommand(cal)
	if err != nil {
		t.Fatalf("buildServeCommand: %v", err)
	}
	args := strings.Join(cmd.Args, " ")
	if strings.Contains(args, "16384") {
		t.Errorf("stale ctx_size 16384 leaked into the launch — must be re-resolved:\n%s", args)
	}
	if !strings.Contains(args, "--ctx-size "+strconv.Itoa(serving.DefaultCtxSize)) {
		t.Errorf("served ctx must be the one default %d; got:\n%s", serving.DefaultCtxSize, args)
	}
}

// TestOllamaCtxUsesOneKnob → REQ-PERF-009: the Ollama-derived model's num_ctx is sourced from the single
// context constant, not a private duplicate that can drift from the llama-server + OpenCode windows.
func TestOllamaCtxUsesOneKnob(t *testing.T) {
	if ollamaCtxTokens != serving.DefaultCtxSize {
		t.Errorf("ollamaCtxTokens = %d; must equal serving.DefaultCtxSize = %d (one ctx knob)", ollamaCtxTokens, serving.DefaultCtxSize)
	}
}
