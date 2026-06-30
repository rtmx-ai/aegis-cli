package main

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

// TestConnectAvailable → REQ-OC-047: an Ollama model resolves to its on-disk GGUF blob (via /api/show
// FROM), so aegis can serve it locally for one-keypress use; a missing blob resolves to "".
func TestConnectAvailable(t *testing.T) {
	blob := filepath.Join(t.TempDir(), "sha256-abc")
	if err := os.WriteFile(blob, []byte("gguf"), 0o644); err != nil {
		t.Fatal(err)
	}
	ok := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, `{"modelfile":"# Modelfile\nFROM %s\nPARAMETER num_ctx 4096\n"}`, blob)
	}))
	defer ok.Close()
	t.Setenv("OLLAMA_HOST", ok.URL)
	if got := ollamaModelGGUF("gemma:7b"); got != blob {
		t.Errorf("must resolve the FROM blob path: got %q want %q", got, blob)
	}
	gone := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"modelfile":"FROM /no/such/blob\n"}`)
	}))
	defer gone.Close()
	t.Setenv("OLLAMA_HOST", gone.URL)
	if ollamaModelGGUF("x") != "" {
		t.Error("a missing blob path must resolve to empty")
	}
}
