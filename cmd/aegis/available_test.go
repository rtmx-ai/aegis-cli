package main

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

// TestBestAvailableModel → REQ-OC-046: the best already-available model is surfaced — a GGUF in the
// model dir wins over an Ollama tag; a working Ollama model is used when no GGUF is present; nil when
// nothing is available (so the screen falls back to recommending a download).
func TestBestAvailableModel(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "my-model.gguf"), make([]byte, 1024), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("MODEL_DOWNLOAD_DIR", dir)
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	if m := bestAvailableModel(); m == nil || m.Kind != "gguf" || m.ID != "my-model.gguf" {
		t.Fatalf("a GGUF in the model dir must be the best available: %+v", m)
	}

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"qwen:7b"}]}`))
		case "/v1/chat/completions":
			w.WriteHeader(http.StatusOK)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("MODEL_DOWNLOAD_DIR", t.TempDir()) // empty — no GGUF
	t.Setenv("OLLAMA_HOST", srv.URL)
	if m := bestAvailableModel(); m == nil || m.Kind != "ollama" || m.ID != "qwen:7b" {
		t.Fatalf("a working Ollama model must be surfaced when no GGUF: %+v", m)
	}

	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	if bestAvailableModel() != nil {
		t.Error("no GGUF + no Ollama → nil")
	}
}
