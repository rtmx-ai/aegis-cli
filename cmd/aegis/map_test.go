package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestMapCommand → the `aegis map` CLI prints a repo map of the target tree — the
// engine behind the /map opencode command (INDEX-001-P05).
func TestMapCommand(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "a.go"), []byte("package p\n\nfunc Exported() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	var out, errb bytes.Buffer
	if rc := cmdMap([]string{"--root", dir}, &out, &errb); rc != 0 {
		t.Fatalf("cmdMap rc=%d err=%s", rc, errb.String())
	}
	if !strings.Contains(out.String(), "Exported") || !strings.Contains(out.String(), "a.go") {
		t.Errorf("map output missing symbol/file; got: %s", out.String())
	}
}
