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
	coder := Aggregate("devstral", "devstral-small", []Outcome{
		{Task: "go-add", FilesEdited: 1, Closed: true, FirstPass: true, ToolCalls: 3, ValidToolCalls: 3, Turns: 2, Tokens: 900, OutTokens: 200, WallMs: 60000},
		{Task: "go-max", FilesEdited: 1, Closed: true, FirstPass: true, ToolCalls: 2, ValidToolCalls: 2, Turns: 2, Tokens: 800, OutTokens: 180, WallMs: 60000},
	})
	// "dry": fast, but narrates — never edits a file, never closes (the reported symptom).
	dry := Aggregate("gemma-dry", "gemma-4-26b", []Outcome{
		{Task: "go-add", FilesEdited: 0, Closed: false, ToolCalls: 1, ValidToolCalls: 0, Turns: 1, Tokens: 300, OutTokens: 120, WallMs: 5000},
		{Task: "go-max", FilesEdited: 0, Closed: false, ToolCalls: 0, ValidToolCalls: 0, Turns: 1, Tokens: 250, OutTokens: 100, WallMs: 5000},
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
	none := Compare("suite-a", "m5-24gb", []CandidateReport{dry, Aggregate("also-dry", "phi-4", []Outcome{{Task: "go-add", FilesEdited: 0}})})
	if none.Winner != "" {
		t.Errorf("no-edit field must yield no winner, got %q", none.Winner)
	}
	if !strings.Contains(none.Basis, "agency bar") {
		t.Errorf("no-winner basis must explain the agency-bar failure: %q", none.Basis)
	}
}

// TestSameServedModelInvalidates → REQ-BENCH-010: the guard that catches the first bake-off's fatal flaw —
// if two candidates were served by the SAME model (the endpoint was never swapped), the head-to-head is
// one model measured twice and must be refused, not ranked. This is what turns a silent bogus "winner"
// into a loud, correct "INVALID".
func TestSameServedModelInvalidates(t *testing.T) {
	// Two "different" candidates that both actually ran on gemma (as happened live).
	a := Aggregate("gemma-4-26b-a4b", "gemma-4-26B-A4B", []Outcome{{Task: "go-add", FilesEdited: 1, Closed: true, OutTokens: 200, WallMs: 30000}})
	b := Aggregate("devstral-small-2507", "gemma-4-26B-A4B", []Outcome{{Task: "go-add", FilesEdited: 1, Closed: true, OutTokens: 200, WallMs: 30000}})
	c := Compare("default", "m5-24gb", []CandidateReport{a, b})
	if c.Winner != "" {
		t.Errorf("same served model must invalidate the comparison, got winner=%q", c.Winner)
	}
	if !strings.Contains(c.Basis, "INVALID") || !strings.Contains(c.Basis, "same model") {
		t.Errorf("basis must call out the same-served-model trap: %q", c.Basis)
	}
	// Distinct served models on the same candidates → a valid ranking is allowed.
	b2 := Aggregate("devstral-small-2507", "Devstral-Small-2507", []Outcome{{Task: "go-add", FilesEdited: 1, Closed: true, OutTokens: 60, WallMs: 90000}})
	if got := Compare("default", "m5-24gb", []CandidateReport{a, b2}); got.Winner == "" {
		t.Errorf("distinct served models must allow a winner; basis=%q", got.Basis)
	}
}
