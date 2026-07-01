package index

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestPolyglotRepoMap → REQ-INDEX-010: Build walks + ranks non-Go files (via the
// INDEX-009 extractor) alongside Go, and personalization boosts a non-Go file.
func TestPolyglotRepoMap(t *testing.T) {
	root := t.TempDir()
	w := func(name, src string) {
		if err := os.WriteFile(filepath.Join(root, name), []byte(src), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	w("svc.go", "package p\n\nfunc Handler() {}\n")
	w("worker.py", "def process():\n    pass\n\nclass Job:\n    pass\n")
	w("engine.rs", "pub fn run() {}\npub struct Engine {}\n")

	out, err := Build(Options{Root: root, TokenBudget: 8000})
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"svc.go", "Handler", "worker.py", "process", "Job", "engine.rs", "run", "Engine"} {
		if !strings.Contains(out, want) {
			t.Errorf("polyglot repo map missing %q:\n%s", want, out)
		}
	}

	// Personalizing on a non-Go symbol boosts its file into a tight budget.
	ranked, err := Build(Options{Root: root, Mentions: []string{"process"}, TokenBudget: 40})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(ranked, "worker.py") {
		t.Errorf("a mentioned Python symbol's file should rank into a tight budget:\n%s", ranked)
	}
}
