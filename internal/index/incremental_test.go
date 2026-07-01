package index

import (
	"os"
	"path/filepath"
	"slices"
	"testing"
)

// TestIncrementalIndex → REQ-INDEX-004: a content-hash snapshot detects exactly the
// files added/modified (changed) and deleted (removed) between two states, so
// re-indexing touches only what changed.
func TestIncrementalIndex(t *testing.T) {
	root := t.TempDir()
	w := func(name, content string) {
		if err := os.WriteFile(filepath.Join(root, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	w("a.go", "package p\n\nfunc A() {}\n")
	w("b.go", "package p\n\nfunc B() {}\n")
	prev := SnapshotDir(root)

	// Modify a.go, add c.go, remove b.go.
	w("a.go", "package p\n\nfunc A() { /* changed */ }\n")
	w("c.go", "package p\n\nfunc C() {}\n")
	if err := os.Remove(filepath.Join(root, "b.go")); err != nil {
		t.Fatal(err)
	}
	cur := SnapshotDir(root)

	changed, removed := ChangedSince(prev, cur)
	if !slices.Contains(changed, "a.go") || !slices.Contains(changed, "c.go") {
		t.Errorf("changed must include a.go (modified) + c.go (added): %v", changed)
	}
	if slices.Contains(changed, "b.go") {
		t.Error("a removed file must not appear in changed")
	}
	if !slices.Contains(removed, "b.go") {
		t.Errorf("removed must include b.go: %v", removed)
	}

	// Identical snapshots -> no work.
	if c2, r2 := ChangedSince(cur, cur); len(c2) != 0 || len(r2) != 0 {
		t.Errorf("identical snapshots must show no change: changed=%v removed=%v", c2, r2)
	}
}
