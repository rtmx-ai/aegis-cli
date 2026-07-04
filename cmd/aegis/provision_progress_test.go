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

// TestProvisionProgressInline → REQ-OC-050: on a terminal the download progress rewrites ONE line in
// place (\r), instead of scrolling a new line every second (the reported long-running output) — while
// still emitting newline-delimited lines when piped, which the in-TUI provisioning screen (OC-022) parses.
func TestProvisionProgressInline(t *testing.T) {
	// TTY mode: partial updates carriage-return in place; completion adds a single trailing newline.
	var tty bytes.Buffer
	pt := &progressTracker{total: 100, w: &tty, tty: true}
	_, _ = pt.Write(make([]byte, 40)) // 40% — first write always emits (last time is zero)
	_, _ = pt.Write(make([]byte, 60)) // 100% — completion
	out := tty.String()
	if !strings.Contains(out, "\r\033[K") {
		t.Errorf("tty progress must rewrite in place with \\r + clear-line; got %q", out)
	}
	if strings.Count(out, "\n") != 1 || !strings.HasSuffix(out, "\n") {
		t.Errorf("tty progress must emit exactly one trailing newline, at completion; got %q", out)
	}

	// Piped mode: newline-delimited lines the TUI parses, no carriage returns.
	var piped bytes.Buffer
	pp := &progressTracker{total: 100, w: &piped, tty: false}
	_, _ = pp.Write(make([]byte, 100))
	po := piped.String()
	if strings.Contains(po, "\r") {
		t.Errorf("piped progress must NOT use carriage returns (the TUI parses lines); got %q", po)
	}
	if !strings.Contains(po, "downloaded ") || !strings.HasSuffix(po, "\n") {
		t.Errorf("piped progress must emit the parseable 'downloaded …' line; got %q", po)
	}
}
