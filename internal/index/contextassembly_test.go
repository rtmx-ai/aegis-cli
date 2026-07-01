package index

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestContextAssembly → REQ-INDEX-005: the assembled bundle carries the repo map
// plus the body of the relevant file, excludes irrelevant file bodies, and respects
// the budget.
func TestContextAssembly(t *testing.T) {
	root := t.TempDir()
	w := func(n, c string) {
		if err := os.WriteFile(filepath.Join(root, n), []byte(c), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	w("widget.go", "package p\n\nfunc Widget() int { return 1 }\n")
	w("other.go", "package p\n\nfunc Other() int { return 2 }\n")

	out := AssembleContext(root, []string{"Widget"}, 8000)
	if !strings.Contains(out, "Repo map") {
		t.Error("assembly must include the repo map")
	}
	if !strings.Contains(out, "return 1") {
		t.Error("assembly must include the relevant file body (Widget)")
	}
	if strings.Contains(out, "return 2") {
		t.Error("an irrelevant file body (Other) must be excluded")
	}

	// Budget is respected (tiny budget yields a small bundle).
	if tiny := AssembleContext(root, []string{"Widget"}, 50); len(tiny) > 300 {
		t.Errorf("budget not respected: %d chars", len(tiny))
	}
}
