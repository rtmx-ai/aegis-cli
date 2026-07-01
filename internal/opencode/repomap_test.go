package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestRepoMapAutoInjected → REQ-INDEX-006: aegis auto-stages the repo map at launch
// so the codebase skeleton is in context automatically (context-builder mode), not
// only on an explicit /map invocation.
func TestRepoMapAutoInjected(t *testing.T) {
	seed := t.TempDir()
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "z.go"), []byte("package p\n\nfunc Zap() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if !StageRepoMap(seed, root) {
		t.Fatal("StageRepoMap did not stage a map")
	}
	b, err := os.ReadFile(filepath.Join(seed, RepoMapFile))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "Zap") || !strings.Contains(string(b), "z.go") {
		t.Errorf("staged repo map missing symbol/file; got:\n%s", b)
	}
	// A tree with no Go source stages nothing (best-effort, launch proceeds).
	if StageRepoMap(t.TempDir(), t.TempDir()) {
		t.Error("StageRepoMap should stage nothing for an empty tree")
	}
}
