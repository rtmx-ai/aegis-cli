package index

import "strings"

// firstClassLanguages is aegis's first-class retrieval language set, kept EQUAL to
// rtmx's supported set by construction (INDEX-008). Beyond this set, retrieval
// degrades to the grep floor (INDEX-003) and verification uses rtmx's universal
// `// rtmx:req` comment marker. A language may only be first-classed here if it is
// also first-classed in rtmx (rtmx from-tests) — aegis never navigates a language
// it cannot close a requirement against. LSP/SCIP coverage that exceeds this set
// (PHP/Elixir/Zig via LSP; Java/Kotlin/Scala via SCIP) stays behind this rule.
var firstClassLanguages = []string{
	"c", "cpp", "csharp", "go", "javascript", "python", "ruby", "rust", "typescript",
}

// langAliases normalizes common spellings/extensions to the canonical language id.
var langAliases = map[string]string{
	"c++": "cpp", "cc": "cpp", "cxx": "cpp", "hpp": "cpp",
	"c#": "csharp", "cs": "csharp",
	"golang": "go",
	"js":     "javascript", "jsx": "javascript", "node": "javascript",
	"ts": "typescript", "tsx": "typescript",
	"py": "python", "py3": "python",
	"rb": "ruby",
	"rs": "rust",
}

// NormalizeLang maps a language name/alias/extension to its canonical id.
func NormalizeLang(lang string) string {
	l := strings.ToLower(strings.TrimSpace(lang))
	l = strings.TrimPrefix(l, ".")
	if c, ok := langAliases[l]; ok {
		return c
	}
	return l
}

// FirstClassLanguages returns aegis's first-class retrieval set (INDEX-008).
func FirstClassLanguages() []string {
	out := make([]string, len(firstClassLanguages))
	copy(out, firstClassLanguages)
	return out
}

// IsFirstClass reports whether a language is in aegis's first-class set; anything
// else degrades to the grep floor (INDEX-008 / INDEX-007).
func IsFirstClass(lang string) bool {
	l := NormalizeLang(lang)
	for _, f := range firstClassLanguages {
		if f == l {
			return true
		}
	}
	return false
}
