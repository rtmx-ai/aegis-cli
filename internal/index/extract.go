package index

import (
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// INDEX-009: pure-Go, ctags-style multi-language definition extraction. Per-language
// regex patterns pull top-level defs into the same Symbol list the repo map consumes
// — no CGO, no WASM runtime, no grammar blobs, no downloads (full out-of-the-box
// static compliance). Lower fidelity than tree-sitter (no full ref graph, best-effort
// on C/C++); tree-sitter (INDEX-001-P01) is the deferred accuracy upgrade. Go keeps
// using go/ast.

type defPattern struct {
	re   *regexp.Regexp
	name int // submatch index holding the symbol name
}

func pat(expr string, name int) defPattern { return defPattern{regexp.MustCompile(expr), name} }

// jsPatterns cover JavaScript and TypeScript.
var jsPatterns = []defPattern{
	pat(`^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+(\w+)`, 1),
	pat(`^\s*(?:export\s+)?(?:abstract\s+)?class\s+(\w+)`, 1),
	pat(`^\s*(?:export\s+)?(?:interface|type|enum)\s+(\w+)`, 1),
	pat(`^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\(?[\w,\s]*\)?\s*=>`, 1),
}

// cPatterns cover C and C++ (best-effort).
var cPatterns = []defPattern{
	pat(`^\s*(?:typedef\s+)?(?:struct|class|enum|union)\s+(\w+)`, 1),
	pat(`^[A-Za-z_][\w\s\*&:<>,]*\s+\*?(\w+)\s*\([^;]*\)\s*\{`, 1),
}

var defExtractors = map[string][]defPattern{
	"python": {
		pat(`^\s*(?:async\s+)?def\s+(\w+)`, 1),
		pat(`^\s*class\s+(\w+)`, 1),
	},
	"ruby": {
		pat(`^\s*def\s+(?:self\.)?(\w+)`, 1),
		pat(`^\s*(?:class|module)\s+(\w+)`, 1),
	},
	"rust": {
		pat(`^\s*(?:pub\s+(?:\([^)]*\)\s+)?)?(?:async\s+)?fn\s+(\w+)`, 1),
		pat(`^\s*(?:pub\s+)?(?:struct|enum|trait|union|type)\s+(\w+)`, 1),
		pat(`^\s*(?:pub\s+)?const\s+(\w+)`, 1),
	},
	"javascript": jsPatterns,
	"typescript": jsPatterns,
	"csharp": {
		pat(`^\s*(?:(?:public|private|protected|internal|static|abstract|sealed|partial)\s+)*(?:class|interface|struct|enum)\s+(\w+)`, 1),
		pat(`^\s*(?:(?:public|private|protected|internal|static|virtual|override|async)\s+)+[\w<>\[\],\.]+\s+(\w+)\s*\(`, 1),
	},
	"c":   cPatterns,
	"cpp": cPatterns,
}

// ExtractSymbols returns top-level definition symbols for a source file's content
// in the given language (INDEX-009). Returns nil for a language without patterns —
// the caller then degrades to the grep tier (INDEX-007).
func ExtractSymbols(lang, content string) []Symbol {
	pats, ok := defExtractors[NormalizeLang(lang)]
	if !ok {
		return nil
	}
	var out []Symbol
	seen := map[string]bool{}
	for _, line := range strings.Split(content, "\n") {
		for _, p := range pats {
			m := p.re.FindStringSubmatch(line)
			if m == nil {
				continue
			}
			name := m[p.name]
			if name == "" || seen[name] {
				continue
			}
			seen[name] = true
			out = append(out, Symbol{Name: name, Sig: strings.TrimSpace(line)})
		}
	}
	return out
}

// StructuralLanguages returns the languages the pure-Go extractor covers (INDEX-009).
func StructuralLanguages() []string {
	out := make([]string, 0, len(defExtractors))
	for l := range defExtractors {
		out = append(out, l)
	}
	sort.Strings(out)
	return out
}

var extToLang = map[string]string{
	".py": "python", ".rb": "ruby", ".rs": "rust",
	".js": "javascript", ".jsx": "javascript", ".mjs": "javascript",
	".ts": "typescript", ".tsx": "typescript",
	".cs": "csharp",
	".c":  "c", ".h": "c",
	".cc": "cpp", ".cpp": "cpp", ".cxx": "cpp", ".hpp": "cpp", ".hh": "cpp",
	".go": "go",
}

// LangFromPath returns the canonical language id for a file path ("" if unknown).
func LangFromPath(path string) string {
	return extToLang[strings.ToLower(filepath.Ext(path))]
}
