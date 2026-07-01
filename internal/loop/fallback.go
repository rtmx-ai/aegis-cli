package loop

// fallbackDirective is injected when the same failure repeats, telling the agent to
// vary its approach rather than repeat the failed edits (LONGRUN-010).
const fallbackDirective = "The previous approach failed repeatedly with the same result. Try a materially different approach — do not repeat the failed edits."

// FallbackPolicy decides whether to make a fallback attempt (a higher-variance
// retry — bumped temperature / the lead quant) before the loop parks, after M
// consecutive identical failures (LONGRUN-010; RA.Aid FallbackHandler, OpenHands
// temp 0->1.0). Zero value is disabled.
type FallbackPolicy struct {
	// AfterFailures is the count of consecutive identical failures that triggers a
	// fallback (0 = disabled).
	AfterFailures int
	// Temperature is the bumped sampling temperature signaled for the fallback.
	Temperature float64
}

// Fallback reports whether to fall back given the count of consecutive identical
// failures, plus the temperature to use.
func (p FallbackPolicy) Fallback(consecutiveIdentical int) (do bool, temp float64) {
	if p.AfterFailures > 0 && consecutiveIdentical >= p.AfterFailures {
		return true, p.Temperature
	}
	return false, 0
}
