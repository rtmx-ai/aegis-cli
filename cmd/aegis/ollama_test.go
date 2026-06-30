package main

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestDetectOllama → REQ-OC-028: aegis probes a running Ollama's tag list and returns its installed
// model names (nil when Ollama is down), so the no-model screen can offer them.
func TestDetectOllama(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/tags" {
			_, _ = w.Write([]byte(`{"models":[{"name":"qwen3-coder:30b"},{"name":"gemma:7b"}]}`))
			return
		}
		w.WriteHeader(http.StatusNotFound)
	}))
	defer srv.Close()

	t.Setenv("OLLAMA_HOST", srv.URL)
	got := detectOllama()
	if len(got) != 2 || got[0] != "qwen3-coder:30b" || got[1] != "gemma:7b" {
		t.Errorf("detectOllama = %v, want [qwen3-coder:30b gemma:7b]", got)
	}

	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1") // nothing listening
	if got := detectOllama(); got != nil {
		t.Errorf("a down Ollama must return nil, got %v", got)
	}
}

// TestOllamaFallback → REQ-OC-028: ollamaFallback points cfg at a running Ollama (endpoint + first
// model) when one is up, and reports false when none is.
func TestOllamaFallback(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"models":[{"name":"llama3:8b"}]}`))
	}))
	defer srv.Close()
	t.Setenv("OLLAMA_HOST", srv.URL)
	oc, models, ok := ollamaFallback(config.Default())
	if !ok || len(models) != 1 || oc.ModelID != "llama3:8b" || oc.Endpoint != srv.URL {
		t.Errorf("ollamaFallback should point cfg at Ollama: ok=%v models=%v endpoint=%q model=%q", ok, models, oc.Endpoint, oc.ModelID)
	}
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	if _, _, ok := ollamaFallback(config.Default()); ok {
		t.Error("no Ollama must give false")
	}
}
