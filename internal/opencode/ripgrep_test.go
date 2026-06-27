package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestRipgrepStaged is the OC-009 acceptance test: a staged rg
// (deploy/opencode/bin/rg) is resolved, and the hardened launch env puts its
// directory first on PATH so OpenCode's which("rg") finds the bundled binary
// instead of fetching ripgrep from github at bootstrap.
func TestRipgrepStaged(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)

	// Before staging: resolution fails and the launch env leaves PATH alone (no
	// override pointing at a non-existent dir).
	if _, ok := ResolveRipgrep(); ok {
		t.Fatal("ResolveRipgrep should report not-found when nothing is staged")
	}
	for _, e := range airgapEnv(config.Default(), true) {
		if strings.HasPrefix(e, "PATH=") {
			t.Fatalf("airgapEnv set PATH with no rg staged: %q", e)
		}
	}

	// Stage a real (executable) rg alongside the staged OpenCode binary.
	staged := filepath.Join(dir, StagedRipgrepRelPath)
	if err := os.MkdirAll(filepath.Dir(staged), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(staged, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	got, ok := ResolveRipgrep()
	if !ok {
		t.Fatal("ResolveRipgrep did not find the staged rg")
	}
	wantAbs, _ := filepath.Abs(staged)
	if got != wantAbs {
		t.Errorf("ResolveRipgrep = %q, want %q", got, wantAbs)
	}

	// The hardened launch env must put the staged rg's directory first on PATH so a
	// PATH lookup of "rg" resolves the bundled binary.
	var pathVal string
	found := false
	for _, e := range airgapEnv(config.Default(), true) {
		if strings.HasPrefix(e, "PATH=") {
			pathVal, found = strings.TrimPrefix(e, "PATH="), true
		}
	}
	if !found {
		t.Fatal("airgapEnv did not set PATH with an rg staged")
	}
	first := strings.SplitN(pathVal, string(os.PathListSeparator), 2)[0]
	if first != filepath.Dir(wantAbs) {
		t.Errorf("hardened PATH first element = %q, want staged rg dir %q", first, filepath.Dir(wantAbs))
	}
}
