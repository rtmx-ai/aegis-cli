package loop

import "testing"

// TestBestOfNSelector → REQ-THINK-005: the test (not a model-judge) selects among
// up to 2 candidates, and the gate spends best-of-N only on hard requirements.
func TestBestOfNSelector(t *testing.T) {
	// The passing candidate is selected — test as selector, not opinion.
	best := SelectBestOfN([]Candidate{{ID: "a", Passed: false}, {ID: "b", Passed: true}})
	if best == nil || best.ID != "b" {
		t.Errorf("must select the test-passing candidate, got %v", best)
	}
	// First passing candidate wins when several pass.
	if b := SelectBestOfN([]Candidate{{ID: "a", Passed: true}, {ID: "b", Passed: true}}); b == nil || b.ID != "a" {
		t.Errorf("first passing candidate should win, got %v", b)
	}
	// None pass -> nil (nothing to select).
	if SelectBestOfN([]Candidate{{ID: "a", Passed: false}}) != nil {
		t.Error("no passing candidate must return nil")
	}

	// Gate: hard -> up to MaxBestOfN, trivial -> 1 (no best-of-N cost).
	if BestOfNGate(true) != MaxBestOfN || MaxBestOfN != 2 {
		t.Errorf("hard requirement gets N=%d (want 2)", BestOfNGate(true))
	}
	if BestOfNGate(false) != 1 {
		t.Error("trivial requirement gets N=1")
	}
}
