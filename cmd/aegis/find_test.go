package main

import (
	"os"
	"path/filepath"
	"testing"
)

// TestProvisionFind → REQ-OC-045: the deep scan finds GGUF models in the search dirs and a filter
// narrows the list.
func TestProvisionFind(t *testing.T) {
	dir := t.TempDir()
	for _, n := range []string{"gemma-4-26b.gguf", "qwen-7b.gguf", "notes.txt"} {
		if err := os.WriteFile(filepath.Join(dir, n), []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	t.Setenv("HOME", t.TempDir())
	t.Setenv("MODEL_DOWNLOAD_DIR", t.TempDir()) // empty
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	t.Setenv("AEGIS_MODEL_PATHS", dir)
	if all := findModels(""); len(all) != 2 {
		t.Errorf("must find both GGUFs (not the .txt), got %d: %+v", len(all), all)
	}
	g := findModels("gemma")
	if len(g) != 1 || g[0].ID != "gemma-4-26b.gguf" {
		t.Errorf("filter must narrow to gemma: %+v", g)
	}
}
