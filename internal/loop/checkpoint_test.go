package loop

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// TestEditCheckpoint → REQ-LONGRUN-007: a shadow-git checkpoint snapshots the
// workspace, and rollback restores it to that point — a bad edit (and any new file)
// is undone without touching the project's own git history.
func TestEditCheckpoint(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not available")
	}
	ws := t.TempDir()
	gitDir := filepath.Join(t.TempDir(), "shadow.git")
	f := filepath.Join(ws, "f.txt")
	if err := os.WriteFile(f, []byte("v1"), 0o644); err != nil {
		t.Fatal(err)
	}

	c1, err := Checkpoint(ws, gitDir)
	if err != nil || c1 == "" {
		t.Fatalf("checkpoint: %v (sha %q)", err, c1)
	}

	// A bad mid-task edit + a new stray file.
	if err := os.WriteFile(f, []byte("v2-BAD"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ws, "stray.txt"), []byte("junk"), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := Rollback(ws, gitDir, c1); err != nil {
		t.Fatalf("rollback: %v", err)
	}
	if b, _ := os.ReadFile(f); string(b) != "v1" {
		t.Errorf("rollback must restore v1, got %q", b)
	}
	if _, err := os.Stat(filepath.Join(ws, "stray.txt")); !os.IsNotExist(err) {
		t.Error("rollback must remove files created after the checkpoint")
	}
}
