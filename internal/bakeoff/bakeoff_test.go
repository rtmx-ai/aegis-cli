package bakeoff

import (
	"strings"
	"testing"
)

// TestAggregateAndCompare → REQ-BENCH-010: the rig turns per-cell outcomes into the agency + throughput
// metrics, and ranks agency-first — a fast model that writes no code must lose to a slower one that edits
// and closes. This is the whole point: "dry/shallow" (edits nothing) and "too slow" become comparable
// numbers, and the ranking refuses to crown a fast do-nothing.
func TestAggregateAndCompare(t *testing.T) {
	// "coder": edits + closes both tasks, but slow (24B dense on a low-bandwidth box).
	coder := Aggregate("devstral", []Outcome{
		{Task: "go-add", FilesEdited: 1, Closed: true, FirstPass: true, ToolCalls: 3, ValidToolCalls: 3, Turns: 2, Tokens: 900, WallMs: 60000},
		{Task: "go-max", FilesEdited: 1, Closed: true, FirstPass: true, ToolCalls: 2, ValidToolCalls: 2, Turns: 2, Tokens: 800, WallMs: 60000},
	})
	// "dry": fast, but narrates — never edits a file, never closes (the reported symptom).
	dry := Aggregate("gemma-dry", []Outcome{
		{Task: "go-add", FilesEdited: 0, Closed: false, ToolCalls: 1, ValidToolCalls: 0, Turns: 1, Tokens: 300, WallMs: 5000},
		{Task: "go-max", FilesEdited: 0, Closed: false, ToolCalls: 0, ValidToolCalls: 0, Turns: 1, Tokens: 250, WallMs: 5000},
	})

	if coder.EditRate != 1.0 {
		t.Errorf("coder EditRate = %v, want 1.0", coder.EditRate)
	}
	if dry.EditRate != 0.0 {
		t.Errorf("dry EditRate = %v, want 0.0 (wrote nothing)", dry.EditRate)
	}
	if coder.Report.ACR != 1.0 || dry.Report.ACR != 0.0 {
		t.Errorf("ACR: coder=%v dry=%v, want 1.0 / 0.0", coder.Report.ACR, dry.Report.ACR)
	}
	// tok/s: dry is faster per token, but that must not win it the bake-off.
	if !(dry.TokPerSec > coder.TokPerSec) {
		t.Errorf("expected the dry model to be faster per token (got coder=%.1f dry=%.1f)", coder.TokPerSec, dry.TokPerSec)
	}

	cmp := Compare("suite-a", "m5-24gb", []CandidateReport{dry, coder})
	if cmp.Winner != "devstral" {
		t.Errorf("winner = %q, want devstral (agency beats speed)", cmp.Winner)
	}
	if !strings.Contains(cmp.Table(), "edited") || !strings.Contains(cmp.Table(), "devstral") {
		t.Errorf("table must render agency columns + candidates:\n%s", cmp.Table())
	}

	// A field where NOBODY edits → no winner (a serving/template failure, not a model ranking).
	none := Compare("suite-a", "m5-24gb", []CandidateReport{dry, Aggregate("also-dry", []Outcome{{Task: "go-add", FilesEdited: 0}})})
	if none.Winner != "" {
		t.Errorf("no-edit field must yield no winner, got %q", none.Winner)
	}
	if !strings.Contains(none.Basis, "agency bar") {
		t.Errorf("no-winner basis must explain the agency-bar failure: %q", none.Basis)
	}
}
