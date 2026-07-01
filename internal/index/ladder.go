package index

// Tier is a retrieval capability level (INDEX-007). Higher = more precise.
type Tier int

const (
	// TierGrep is the universal text floor (INDEX-003) — any language, always.
	TierGrep Tier = iota
	// TierStructural is a def/ref skeleton (go/ast today; tree-sitter/WASM once
	// INDEX-001-P01 lands).
	TierStructural
	// TierPrecise is compiler-grade go-to-def / find-refs (LSP / SCIP).
	TierPrecise
)

// String renders the tier so the active retrieval level is observable, never a
// silent downgrade (INDEX-007).
func (t Tier) String() string {
	switch t {
	case TierPrecise:
		return "precise"
	case TierStructural:
		return "structural"
	default:
		return "grep"
	}
}

// Capabilities declares which languages have a bundled precise server (LSP/SCIP)
// and which have an embedded structural grammar, so the ladder picks a tier from
// what is actually shipped — never assuming coverage that isn't bundled. Populated
// per the INDEX-008 parity rule (first-class languages only).
type Capabilities struct {
	Precise    map[string]bool
	Structural map[string]bool
}

// RetrievalTier returns the best available tier for a language, never below Grep
// (INDEX-007). An unknown or unsupported language resolves to Grep — never an
// error — so development in an unindexed language still works, at the text floor.
func RetrievalTier(lang string, caps Capabilities) Tier {
	l := NormalizeLang(lang)
	if caps.Precise[l] {
		return TierPrecise
	}
	if caps.Structural[l] {
		return TierStructural
	}
	return TierGrep
}

// DefaultCapabilities reports what aegis ships TODAY: Go via go/ast plus the
// INDEX-009 pure-Go ctags-style extractor give every first-class language a
// structural tier; no precise servers are bundled yet (LSP/SCIP deferred), so the
// precise tier is currently empty.
func DefaultCapabilities() Capabilities {
	structural := map[string]bool{"go": true}
	for _, l := range StructuralLanguages() {
		structural[l] = true
	}
	return Capabilities{
		Precise:    map[string]bool{},
		Structural: structural,
	}
}
