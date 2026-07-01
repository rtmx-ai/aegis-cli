package loop

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/memory"
)

// TestResearchPreStage → REQ-LONGRUN-011: the bounded discovery pass greps the task
// terms over the workspace and emits file:line snippets into the working-memory
// store, so planning starts from curated context rather than raw file dumps.
func TestResearchPreStage(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "a.go"), []byte("package p\n\nfunc Widget() {}\nvar other = 1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "b.go"), []byte("package p\n\n// uses Widget here\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	store, err := memory.Open(filepath.Join(t.TempDir(), "mem.json"), 50, 50)
	if err != nil {
		t.Fatal(err)
	}

	n := ResearchPreStage(store, root, []string{"Widget"}, 10)
	if n < 2 {
		t.Fatalf("want >=2 Widget hits, got %d", n)
	}
	q := store.Query("Widget")
	if len(q) < 2 {
		t.Errorf("store should hold Widget snippets, got %d", len(q))
	}
	keyed := false
	for _, e := range q {
		if strings.Contains(e.Key, ".go:") {
			keyed = true
		}
	}
	if !keyed {
		t.Error("snippets should be keyed file:line")
	}
	// No terms is a no-op.
	if ResearchPreStage(store, root, nil, 10) != 0 {
		t.Error("no terms must be a no-op")
	}
}
