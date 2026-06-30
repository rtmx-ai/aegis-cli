package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestModelGardenOverride → REQ-OC-040: AEGIS_MODEL_GARDEN rewrites the download host (keeping the
// pinned filename + sha256), and AEGIS_CATALOG points aegis at an operator-supplied catalog.
func TestModelGardenOverride(t *testing.T) {
	t.Setenv("AEGIS_MODEL_GARDEN", "https://mirror.corp.example/models/")
	if got := modelGardenURL("gemma.gguf", "https://github.com/x/y/gemma.gguf"); got != "https://mirror.corp.example/models/gemma.gguf" {
		t.Errorf("garden override = %q (want the mirror host + pinned filename)", got)
	}
	t.Setenv("AEGIS_MODEL_GARDEN", "")
	if got := modelGardenURL("f.gguf", "https://orig/f.gguf"); got != "https://orig/f.gguf" {
		t.Errorf("no override must keep the catalog URL, got %q", got)
	}
	dir := t.TempDir()
	cat := filepath.Join(dir, "catalog.json")
	if err := os.WriteFile(cat, []byte(`{"models":[{"id":"corp-model"}]}`), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("AEGIS_CATALOG", cat)
	b, err := catalogBytes()
	if err != nil || !strings.Contains(string(b), "corp-model") {
		t.Errorf("AEGIS_CATALOG must be read: err=%v body=%q", err, string(b))
	}
}
