package index

import "testing"

// TestLanguageParity → REQ-INDEX-008: aegis's first-class retrieval set equals
// rtmx's supported set; aliases normalize into it; anything beyond it is not
// first-class (degrades to grep) and never errors.
func TestLanguageParity(t *testing.T) {
	// Mirrors rtmx's supported languages (rtmx from-tests): Go, Python, Rust,
	// JavaScript, TypeScript, C#, C, C++, Ruby. If aegis and rtmx drift, this fails.
	rtmxSet := map[string]bool{
		"c": true, "cpp": true, "csharp": true, "go": true, "javascript": true,
		"python": true, "ruby": true, "rust": true, "typescript": true,
	}
	fc := FirstClassLanguages()
	if len(fc) != len(rtmxSet) {
		t.Fatalf("first-class set size %d != rtmx set size %d (%v)", len(fc), len(rtmxSet), fc)
	}
	for _, l := range fc {
		if !rtmxSet[l] {
			t.Errorf("%q is first-class in aegis but not in rtmx's set (parity broken)", l)
		}
		delete(rtmxSet, l)
	}
	if len(rtmxSet) != 0 {
		t.Errorf("rtmx languages missing from aegis first-class set: %v", rtmxSet)
	}

	// Aliases / extensions normalize into the first-class set.
	for _, alias := range []string{"C++", "c#", "JS", "ts", ".py", "golang", "rb", "rs"} {
		if !IsFirstClass(alias) {
			t.Errorf("alias %q should normalize to a first-class language", alias)
		}
	}
	// Languages beyond rtmx's set (some covered by LSP/SCIP) are NOT first-classed;
	// they degrade to grep, never error.
	for _, beyond := range []string{"php", "elixir", "zig", "java", "kotlin", "cobol", ""} {
		if IsFirstClass(beyond) {
			t.Errorf("%q must NOT be first-class (parity: rtmx doesn't first-class it)", beyond)
		}
	}
}
