package loop

// MaxBestOfN caps best-of-N at 2 candidates (THINK-005). Larger N is on the AVOID
// list for a small local model: the cost outruns the benefit, and self-consistency
// voting is a poor selector. Here the SELECTOR is the requirement's test, not a
// model-judge — execution beats opinion for a weak model.
const MaxBestOfN = 2

// Candidate is one generated attempt with its test outcome (THINK-005).
type Candidate struct {
	ID     string
	Passed bool // did the requirement's test pass for this candidate?
	Patch  string
}

// SelectBestOfN returns the first candidate whose TEST passed, or nil if none did.
// The test is the selector — deterministic, not a model opinion (THINK-005).
func SelectBestOfN(cands []Candidate) *Candidate {
	for i := range cands {
		if cands[i].Passed {
			return &cands[i]
		}
	}
	return nil
}

// BestOfNGate returns how many candidates to generate: up to MaxBestOfN for a hard
// requirement, else 1 — gating the best-of-N spend to where it can pay off, since a
// small model can't afford it broadly (THINK-005).
func BestOfNGate(hard bool) int {
	if hard {
		return MaxBestOfN
	}
	return 1
}
