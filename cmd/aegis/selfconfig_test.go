package main

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

const gib = uint64(1) << 30

// TestSelfConfigFitsCtxToHost → REQ-SERVE-023: aegis sizes the served context to fit the actual host
// memory for the actual model, every launch. A big model on a small box must auto-downshift below the
// desired window instead of serving 32k and OOMing; a roomy box keeps the full window; and a model that
// clearly fits is never shrunk. This is the launch-time self-configuration that makes aegis "just fit"
// on a 24 GB Mac or a 128 GB Mac without manual tuning or stale state.
func TestSelfConfigFitsCtxToHost(t *testing.T) {
	const q4 = "Devstral-Small-2507-Q4_K_M.gguf" // filename signals q4 → ~0.58 bytes/param

	// A 24B-class Q4 model (~14 GiB) on a 24 GB box must downshift well below 32k but stay usable.
	got := fitCtxTokens(serving.DefaultCtxSize, 14*gib, 24*gib, q4)
	if got >= serving.DefaultCtxSize {
		t.Errorf("24 GB host + 14 GiB model must downshift below %d; got %d", serving.DefaultCtxSize, got)
	}
	if got < 4096 {
		t.Errorf("downshift must stay usable (>=4096); got %d", got)
	}

	// The same model on a 128 GB box has ample room — keep the full desired window.
	if got := fitCtxTokens(serving.DefaultCtxSize, 14*gib, 128*gib, q4); got != serving.DefaultCtxSize {
		t.Errorf("128 GB host must keep the full %d window; got %d", serving.DefaultCtxSize, got)
	}

	// A small model that clearly fits is never shrunk.
	if got := fitCtxTokens(serving.DefaultCtxSize, 3*gib, 24*gib, "small-Q4.gguf"); got != serving.DefaultCtxSize {
		t.Errorf("a small model that fits must keep the full window; got %d", got)
	}

	// fitCtxTokens never UPSIZES past the desired ceiling.
	if got := fitCtxTokens(8192, 3*gib, 128*gib, "small-Q4.gguf"); got != 8192 {
		t.Errorf("must never exceed the desired ceiling 8192; got %d", got)
	}

	// Unknown RAM or size → don't second-guess the resolver.
	if got := fitCtxTokens(serving.DefaultCtxSize, 0, 24*gib, q4); got != serving.DefaultCtxSize {
		t.Errorf("unknown model size must return the desired window; got %d", got)
	}
	if got := fitCtxTokens(serving.DefaultCtxSize, 14*gib, 0, q4); got != serving.DefaultCtxSize {
		t.Errorf("unknown RAM must return the desired window; got %d", got)
	}
}

// TestSelfConfigHonorsOperatorOverride → REQ-SERVE-023: an explicit AEGIS_CTX_SIZE means the operator
// takes responsibility for the fit, so the memory cap is skipped entirely.
func TestSelfConfigHonorsOperatorOverride(t *testing.T) {
	// Stage a fake large model file so fitCtxSize would otherwise downshift.
	dir := t.TempDir()
	big := filepath.Join(dir, "big-Q4.gguf")
	if err := os.WriteFile(big, make([]byte, 1<<20), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("AEGIS_CTX_SIZE", "32768")
	if got := fitCtxSize(20000, big); got != 20000 {
		t.Errorf("an explicit AEGIS_CTX_SIZE must bypass the memory cap; got %d want 20000", got)
	}
}
