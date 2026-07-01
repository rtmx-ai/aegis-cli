package opencode

import "strings"

// planDirective seeds the opencode Plan agent: a short, concrete plan before any
// edit, for a non-trivial requirement (LONGRUN-004).
const planDirective = "Plan first: before editing, write a short numbered plan (files to touch, the test that will prove it, the order of steps). Then switch to building. Keep the plan to a few lines."

// PlanFirst decides whether a requirement warrants an explicit plan phase (the
// opencode Plan agent) before editing with the Build agent — LONGRUN-004. Trivial
// requirements skip planning (the overhead isn't worth it for a small model);
// non-trivial ones (multi-week effort or compound, multi-step titles) plan first.
func PlanFirst(effortWeeks float64, title string) bool {
	if effortWeeks >= 1.0 {
		return true
	}
	t := strings.ToLower(title)
	if strings.Count(t, " and ") >= 2 { // three-plus clauses => multi-step
		return true
	}
	if strings.Contains(title, ";") {
		return true
	}
	return false
}
