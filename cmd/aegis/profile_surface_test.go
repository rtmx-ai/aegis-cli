package main

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/profile"
)

func TestProfileHintAndCache(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if h := profileHint("gemma"); h != "" {
		t.Errorf("no cache → no hint, got %q", h)
	}
	rec := profile.Recommendation{
		Fits:        []profile.ModelFit{{ID: "big-model"}},
		Interactive: "big-model",
		Unattended:  "big-model",
	}
	if err := writeProfileCache(rec); err != nil {
		t.Fatal(err)
	}
	if h := profileHint("small-model"); !strings.Contains(h, "big-model") || !strings.Contains(h, "small-model") {
		t.Errorf("upgrade hint should name both the best fit + the running model, got %q", h)
	}
	if h := profileHint("big-model"); !strings.Contains(h, "big-model") || strings.Contains(h, "you're running") {
		t.Errorf("running the best fit → confirmation, no upgrade nudge, got %q", h)
	}
	got, err := loadProfileCache()
	if err != nil || got == nil || got.Interactive != "big-model" {
		t.Errorf("cache round-trip failed: %v %v", got, err)
	}
}

func TestAutoProfileCaches(t *testing.T) {
	root, err := filepath.Abs("../..")
	if err != nil {
		t.Fatal(err)
	}
	t.Chdir(root)
	t.Setenv("HOME", t.TempDir())
	autoProfile()
	if _, lerr := loadProfileCache(); lerr != nil {
		t.Errorf("autoProfile must write a cache: %v", lerr)
	}
}
