package opencode

import (
	"path/filepath"
	"strings"
	"testing"
)

// TestTaskScratchpad → REQ-MEM-001: the task scratchpad is an append-only notes file
// — notes accumulate in order, render for injection, and survive reload (resume).
func TestTaskScratchpad(t *testing.T) {
	sp := Scratchpad{Path: filepath.Join(t.TempDir(), "scratch.md")}
	if sp.Render() != "" {
		t.Error("an empty scratchpad must render empty")
	}
	if err := sp.Append("discovered X"); err != nil {
		t.Fatal(err)
	}
	if err := sp.Append("decided Y"); err != nil {
		t.Fatal(err)
	}
	if err := sp.Append("   "); err != nil { // blank note is a no-op
		t.Fatal(err)
	}

	r := sp.Render()
	if !strings.Contains(r, "discovered X") || !strings.Contains(r, "decided Y") {
		t.Errorf("scratchpad should hold both notes; got:\n%s", r)
	}
	if strings.Index(r, "discovered X") > strings.Index(r, "decided Y") {
		t.Error("notes must be append-only (in original order)")
	}

	// Survives reload (a fresh handle over the same path).
	if !strings.Contains((Scratchpad{Path: sp.Path}).Render(), "discovered X") {
		t.Error("scratchpad did not survive reload")
	}
}
