package bakeoff

import (
	"strings"
	"testing"
)

// TestPaletteAndColoredTable → REQ-BENCH-013: the table renders as a readable, color-coded block on a
// terminal (winner starred + green, agency columns first, served model as a basename), and as clean
// plain text when color is off (piped / NO_COLOR) so nothing leaks ANSI into a file or a parser.
func TestPaletteAndColoredTable(t *testing.T) {
	// Palette: off is an exact passthrough; on wraps in ANSI.
	if got := NewPalette(false).Green("x"); got != "x" {
		t.Errorf("palette off must pass through, got %q", got)
	}
	if got := NewPalette(true).Green("x"); !strings.Contains(got, "\033[32m") || !strings.Contains(got, "\033[0m") {
		t.Errorf("palette on must wrap in ANSI, got %q", got)
	}

	cmp := Compare("default", "m5-24gb", []CandidateReport{
		Aggregate("gemma-4-26b-a4b", "/Users/x/models/gemma-4-26B-A4B.gguf", []Outcome{{Task: "go-add", FilesEdited: 1, Closed: true, OutTokens: 200, WallMs: 30000}}),
		Aggregate("devstral-small-2507", "/Users/x/models/Devstral-Small-2507-IQ4_XS.gguf", []Outcome{{Task: "go-add", FilesEdited: 1, Closed: true, OutTokens: 60, WallMs: 90000}}),
	})

	plain := cmp.Table(false)
	if strings.Contains(plain, "\033[") {
		t.Errorf("color=false must emit NO ANSI escapes:\n%q", plain)
	}
	// served-as is a basename, not the full path (readability + makes the same-model trap obvious).
	if strings.Contains(plain, "/Users/x/models/") {
		t.Errorf("served-as must be a basename, not the full path:\n%s", plain)
	}
	if !strings.Contains(plain, "gemma-4-26B-A4B.gguf") {
		t.Errorf("served-as basename missing:\n%s", plain)
	}

	if col := cmp.Table(true); !strings.Contains(col, "\033[") {
		t.Errorf("color=true must emit ANSI escapes:\n%q", col)
	}
}
