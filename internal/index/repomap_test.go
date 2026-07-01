package index

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeFile(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// TestRepoMap → REQ-INDEX-001: Build produces a ranked, token-budgeted skeleton of
// a Go repo (exported defs per file), ranked by personalized PageRank over the file
// dependency graph, elided into the budget. Zero model, zero network.
func TestRepoMap(t *testing.T) {
	dir := t.TempDir()
	writeFile(t, dir, "a.go", "package p\n\nfunc Foo() int { return 1 }\n")
	writeFile(t, dir, "b.go", "package p\n\nfunc Bar() int { return Foo() }\n")                         // refs Foo (a)
	writeFile(t, dir, "c.go", "package p\n\ntype Widget struct{}\n\nfunc Use() int { return Bar() }\n") // refs Bar (b)

	out, err := Build(Options{Root: dir, Mentions: []string{"Bar"}, TokenBudget: 4000})
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"a.go", "b.go", "c.go", "Foo", "Bar", "Widget"} {
		if !strings.Contains(out, want) {
			t.Errorf("repo map missing %q; got:\n%s", want, out)
		}
	}
	// Personalizing on "Bar" boosts b.go (defines Bar) above c.go (only references it).
	if strings.Index(out, "b.go") > strings.Index(out, "c.go") {
		t.Errorf("personalized on Bar: b.go should rank before c.go; got:\n%s", out)
	}
	// The token budget is respected — Build stops early instead of dumping everything.
	tiny, err := Build(Options{Root: dir, TokenBudget: 20})
	if err != nil {
		t.Fatal(err)
	}
	if len(tiny) > 120 {
		t.Errorf("token budget (20) not respected: %d chars:\n%s", len(tiny), tiny)
	}
}
