package opencode

import (
	"path/filepath"
	"strings"
	"testing"
)

// TestNoAutoLearnMemory → REQ-MEM-004: human-authored intent files are protected —
// machine-written memory cannot rewrite them — while aegis's own machine artifacts
// are writable, and the Scratchpad writer enforces the guard live.
func TestNoAutoLearnMemory(t *testing.T) {
	// Human intent files are protected (by base name, at any depth).
	for _, p := range []string{"CLAUDE.md", "AGENTS.md", "/repo/AGENTS.md", "sub/dir/CLAUDE.md", ".clinerules"} {
		if !IsProtectedIntentFile(p) {
			t.Errorf("%q must be protected", p)
		}
		if GuardIntentWrite(p) == nil {
			t.Errorf("GuardIntentWrite must reject %q", p)
		}
	}
	// aegis's machine-written artifacts are NOT intent files (safe to write).
	for _, p := range []string{RepoMapFile, "repo-map.md", "scratch.md", "mem.json", "REQ-A-001.md", "notes.txt"} {
		if IsProtectedIntentFile(p) {
			t.Errorf("%q must not be treated as protected intent", p)
		}
		if GuardIntentWrite(p) != nil {
			t.Errorf("GuardIntentWrite must allow %q", p)
		}
	}
	// The Scratchpad writer enforces the guard live: appending to a protected path fails.
	if err := (Scratchpad{Path: filepath.Join(t.TempDir(), "CLAUDE.md")}).Append("sneaky"); err == nil {
		t.Error("Scratchpad.Append must refuse a protected intent path (MEM-004)")
	} else if !strings.Contains(err.Error(), "MEM-004") {
		t.Errorf("guard error should cite MEM-004: %v", err)
	}
}
