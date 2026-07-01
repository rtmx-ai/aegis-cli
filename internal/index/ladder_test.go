package index

import "testing"

// TestRetrievalLadder → REQ-INDEX-007: pick the best available tier per language
// (Precise -> Structural -> Grep), fall back to Grep for anything unsupported
// (never error), and render the tier for observability.
func TestRetrievalLadder(t *testing.T) {
	caps := Capabilities{
		Precise:    map[string]bool{"go": true},
		Structural: map[string]bool{"go": true, "python": true},
	}
	if RetrievalTier("go", caps) != TierPrecise {
		t.Error("go with a precise server must resolve to Precise")
	}
	if RetrievalTier("Py", caps) != TierStructural { // alias + grammar only
		t.Error("python with only a grammar must resolve to Structural")
	}
	if RetrievalTier("cobol", caps) != TierGrep {
		t.Error("an unsupported language must resolve to Grep")
	}
	if RetrievalTier("", caps) != TierGrep {
		t.Error("empty language must resolve to Grep (no error)")
	}
	// Never below the grep floor, even with no capabilities.
	if RetrievalTier("go", Capabilities{}) != TierGrep {
		t.Error("no capabilities must still yield the Grep floor")
	}

	// The tier is observable (surfaced, not silent).
	if TierGrep.String() != "grep" || TierStructural.String() != "structural" || TierPrecise.String() != "precise" {
		t.Error("tiers must render for observability")
	}

	// What aegis ships today is honest: Go=structural, others=grep.
	d := DefaultCapabilities()
	if RetrievalTier("go", d) != TierStructural {
		t.Error("today Go has the go/ast structural tier")
	}
	if RetrievalTier("rust", d) != TierGrep {
		t.Error("today Rust degrades to grep (awaits INDEX-001-P01)")
	}
}
