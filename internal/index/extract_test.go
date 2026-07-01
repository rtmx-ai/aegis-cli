package index

import "testing"

// TestMultiLangExtract → REQ-INDEX-009: the pure-Go ctags-style extractor pulls
// top-level defs per first-class language, returns nil for the unsupported (degrade
// to grep), and lifts every first-class language to the Structural tier.
func TestMultiLangExtract(t *testing.T) {
	cases := []struct{ lang, src, want string }{
		{"python", "def foo():\n    pass\n", "foo"},
		{"python", "class Bar:\n", "Bar"},
		{"ruby", "def greet\nend\n", "greet"},
		{"ruby", "module M\nend\n", "M"},
		{"rust", "pub fn run() {}\n", "run"},
		{"rust", "struct Widget {}\n", "Widget"},
		{"javascript", "export function handler() {}\n", "handler"},
		{"typescript", "interface Opts {}\n", "Opts"},
		{"typescript", "export const go = async () => {}\n", "go"},
		{"csharp", "public class Widget {}\n", "Widget"},
		{"cpp", "struct Point {};\n", "Point"},
		{"c", "int main(int argc) {\n", "main"},
	}
	for _, c := range cases {
		if !hasSym(ExtractSymbols(c.lang, c.src), c.want) {
			t.Errorf("%s: expected symbol %q, got %v", c.lang, c.want, ExtractSymbols(c.lang, c.src))
		}
	}

	// Unsupported language -> nil (caller degrades to grep).
	if ExtractSymbols("cobol", "IDENTIFICATION DIVISION.") != nil {
		t.Error("unsupported language must yield nil")
	}
	// Extension -> language mapping.
	if LangFromPath("a/b/foo.py") != "python" || LangFromPath("x.rs") != "rust" || LangFromPath("z.unknown") != "" {
		t.Error("LangFromPath extension mapping")
	}

	// Every first-class language now resolves to the Structural tier.
	caps := DefaultCapabilities()
	for _, l := range FirstClassLanguages() {
		if RetrievalTier(l, caps) != TierStructural {
			t.Errorf("%s should be at the Structural tier now (INDEX-009)", l)
		}
	}
}

func hasSym(syms []Symbol, name string) bool {
	for _, s := range syms {
		if s.Name == name {
			return true
		}
	}
	return false
}
