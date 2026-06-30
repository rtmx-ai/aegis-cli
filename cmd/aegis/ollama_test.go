package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

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
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	if got := detectOllama(); got != nil {
		t.Errorf("a down Ollama must return nil, got %v", got)
	}
}

func TestOllamaFallback(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"llama3:8b"}]}`))
		case "/api/create":
			_, _ = w.Write([]byte(`{"status":"success"}`))
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	t.Setenv("OLLAMA_HOST", srv.URL)
	oc, models, ok := ollamaFallback(config.Default())
	if !ok || len(models) != 1 || oc.ModelID != "aegis-llama3-8b" || oc.Endpoint != srv.URL {
		t.Errorf("ollamaFallback should point cfg at Ollama with a ctx-sized model: ok=%v models=%v endpoint=%q model=%q", ok, models, oc.Endpoint, oc.ModelID)
	}
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	if _, _, ok := ollamaFallback(config.Default()); ok {
		t.Error("no Ollama must give false")
	}
}

func TestOllamaCtxModel(t *testing.T) {
	var created map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"phi4-mini:latest"}]}`))
		case "/api/create":
			_ = json.NewDecoder(r.Body).Decode(&created)
			_, _ = w.Write([]byte(`{"status":"success"}`))
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	t.Setenv("OLLAMA_HOST", srv.URL)
	got := ensureOllamaCtxModel("phi4-mini:latest")
	if got != "aegis-phi4-mini-latest" {
		t.Errorf("derived model = %q, want aegis-phi4-mini-latest", got)
	}
	if created == nil || created["from"] != "phi4-mini:latest" {
		t.Errorf("create must derive FROM the base model, got %v", created)
	}
	if p, _ := created["parameters"].(map[string]any); p == nil || p["num_ctx"] == nil {
		t.Errorf("create must set num_ctx, got parameters %v", created["parameters"])
	}
}
