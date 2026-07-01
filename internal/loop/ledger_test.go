package loop

import (
	"strings"
	"testing"
)

// TestTodoLedger → REQ-LONGRUN-003: the on-disk sub-task ledger is seeded from the
// requirement, accepts add/check, and survives a fresh load (resume) with progress
// intact; re-seed is idempotent.
func TestTodoLedger(t *testing.T) {
	dir := t.TempDir()
	l := Ledger{Dir: dir}
	if err := l.Seed("REQ-A-001", "Do the thing"); err != nil {
		t.Fatal(err)
	}
	if items, _ := l.Items("REQ-A-001"); len(items) != 3 {
		t.Fatalf("seed should create 3 items, got %d", len(items))
	}
	if err := l.Add("REQ-A-001", "extra step"); err != nil {
		t.Fatal(err)
	}
	if err := l.Check("REQ-A-001", 0); err != nil {
		t.Fatal(err)
	}

	// Resume: a fresh Ledger over the same dir sees the persisted state.
	l2 := Ledger{Dir: dir}
	items, _ := l2.Items("REQ-A-001")
	if len(items) != 4 {
		t.Fatalf("after add: want 4 items, got %d", len(items))
	}
	if !items[0].Done {
		t.Error("checked item did not persist across resume")
	}
	if items[3].Text != "extra step" {
		t.Errorf("added item text wrong: %q", items[3].Text)
	}
	// Re-seed is idempotent — preserves the existing checklist and its progress.
	if err := l2.Seed("REQ-A-001", "Do the thing"); err != nil {
		t.Fatal(err)
	}
	if it, _ := l2.Items("REQ-A-001"); len(it) != 4 || !it[0].Done {
		t.Error("re-seed must preserve the existing checklist + progress")
	}
	if !strings.Contains(l2.Render("REQ-A-001"), "Do the thing") {
		t.Error("render missing the title")
	}
}

// TestLedgerInjectedIntoDrive covers the loop wiring: with LedgerDir set, the ledger
// is seeded on claim and re-injected into the drive context every turn.
func TestLedgerInjectedIntoDrive(t *testing.T) {
	dir := t.TempDir()
	rt := rtmxWithPassing("A-001")
	h := &fbHarness{Fake: harnessFake()}
	l, err := New(testCfg(), Deps{RTMX: rt, Harness: h, LedgerDir: dir})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := l.Run(ctx(), true); err != nil {
		t.Fatalf("run: %v", err)
	}
	if len(h.feedbacks) == 0 || !strings.Contains(h.feedbacks[0], "A-001") {
		t.Errorf("drive context should carry the seeded ledger; got %v", h.feedbacks)
	}
}
