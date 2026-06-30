package main

import (
	"bytes"
	"strings"
	"testing"
)

// TestProgressTrackerBar → REQ-OC-034: the download progress is a true bar with a percentage; the
// final write always emits a 100%-filled bar (so the TUI + CLI never stop short of complete).
func TestProgressTrackerBar(t *testing.T) {
	var buf bytes.Buffer
	pt := &progressTracker{total: 1000, w: &buf}
	if _, err := pt.Write(make([]byte, 1000)); err != nil {
		t.Fatal(err)
	}
	out := buf.String()
	if !strings.Contains(out, "(100%)") {
		t.Errorf("final progress must report 100%%: %q", out)
	}
	if !strings.Contains(out, "█") || strings.Contains(out, "░") {
		t.Errorf("a 100%% bar must be fully filled (no empty cells): %q", out)
	}
	// the TUI's parser must still find the "downloaded X/Y GB (Z%)" prefix it keys on
	if !strings.HasPrefix(strings.TrimPrefix(out, "aegis: provision: "), "downloaded ") {
		t.Errorf("TUI-parseable prefix changed: %q", out)
	}
}
