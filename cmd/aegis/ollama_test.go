package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

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
	if got := detectOllama(); len(got) != 2 || got[0] != "qwen3-coder:30b" || got[1] != "gemma:7b" {
		t.Errorf("detectOllama = %v", got)
	}
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	if got := detectOllama(); got != nil {
		t.Errorf("a down Ollama must return nil, got %v", got)
	}
}

// ollamaMux builds a mock Ollama; v1Status is the status returned by /v1/chat/completions (the probe).
func ollamaMux(v1Status int, created *map[string]any) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"llama3:8b"}]}`))
		case "/api/create":
			if created != nil {
				_ = json.NewDecoder(r.Body).Decode(created)
			}
			_, _ = w.Write([]byte(`{"status":"success"}`))
		case "/v1/chat/completions":
			w.WriteHeader(v1Status)
		case "/api/delete":
			w.WriteHeader(http.StatusOK)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	})
}

func TestOllamaFallback(t *testing.T) {
	srv := httptest.NewServer(ollamaMux(http.StatusOK, nil)) // derived model probes OK
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("OLLAMA_HOST", srv.URL)
	oc, models, ok := ollamaFallback(config.Default())
	if !ok || len(models) != 1 || oc.ModelID != "aegis-llama3-8b" || oc.Endpoint != srv.URL {
		t.Errorf("a healthy derived model should be used: ok=%v models=%v model=%q", ok, models, oc.ModelID)
	}
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1")
	if _, _, ok := ollamaFallback(config.Default()); ok {
		t.Error("no Ollama must give false")
	}
}

func TestOllamaFallbackBrokenDerived(t *testing.T) {
	// the derived ctx model load-hangs (500), but the base generates fine (200) -> fall back to the base.
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"llama3:8b"}]}`))
		case "/api/create":
			_, _ = w.Write([]byte(`{"status":"success"}`))
		case "/v1/chat/completions":
			var req struct {
				Model string `json:"model"`
			}
			_ = json.NewDecoder(r.Body).Decode(&req)
			if strings.HasPrefix(req.Model, "aegis-") {
				w.WriteHeader(http.StatusInternalServerError) // derived hangs
			} else {
				w.WriteHeader(http.StatusOK) // base works
			}
		case "/api/delete":
			w.WriteHeader(http.StatusOK)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("OLLAMA_HOST", srv.URL)
	oc, _, ok := ollamaFallback(config.Default())
	if !ok || oc.ModelID != "llama3:8b" {
		t.Errorf("a hanging derived model must fall back to the working base: ok=%v model=%q", ok, oc.ModelID)
	}
	if !ollamaCtxBroken("llama3:8b") {
		t.Error("a failed derived probe must mark the base so it is not recreated next launch")
	}
}

// TestOllamaFallbackUnusableModel -> REQ-OC-035: a model that loads but crashes the backend on
// generation (Gemma 3n: HTTP 500) must NOT be handed to opencode — ollamaFallback reports no usable
// Ollama so the caller shows the provisioning screen instead of a frozen TUI.
func TestOllamaFallbackUnusableModel(t *testing.T) {
	srv := httptest.NewServer(ollamaMux(http.StatusInternalServerError, nil)) // every generation crashes
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("OLLAMA_HOST", srv.URL)
	if _, _, ok := ollamaFallback(config.Default()); ok {
		t.Error("a backend-crashing model must make ollamaFallback report no usable Ollama (ok=false)")
	}
	if !ollamaModelUnusable("llama3:8b") {
		t.Error("OC-036: a crashing model must be marked unusable so the next launch skips it")
	}
}

// TestOllamaSkipsMarkedUnusable -> REQ-OC-036: a model marked unusable is skipped on the next launch —
// aegis uses the next usable model instead of re-probing the bad one for 30s.
func TestOllamaSkipsMarkedUnusable(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"bad:1"},{"name":"good:1"}]}`))
		case "/api/create":
			_, _ = w.Write([]byte(`{"status":"success"}`))
		case "/v1/chat/completions":
			w.WriteHeader(http.StatusOK)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("OLLAMA_HOST", srv.URL)
	markOllamaModelUnusable("bad:1")
	oc, _, ok := ollamaFallback(config.Default())
	if !ok || oc.ModelID != "aegis-good-1" {
		t.Errorf("a marked-unusable model must be skipped for the next usable one: ok=%v model=%q", ok, oc.ModelID)
	}
}

func TestOllamaCtxModel(t *testing.T) {
	var created map[string]any
	srv := httptest.NewServer(ollamaMux(http.StatusOK, &created))
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("OLLAMA_HOST", srv.URL)
	if got := ensureOllamaCtxModel("llama3:8b"); got != "aegis-llama3-8b" {
		t.Errorf("derived model = %q", got)
	}
	if created == nil || created["from"] != "llama3:8b" {
		t.Errorf("create must derive FROM the base, got %v", created)
	}
	if p, _ := created["parameters"].(map[string]any); p == nil || p["num_ctx"] == nil {
		t.Errorf("create must set num_ctx, got %v", created["parameters"])
	}
}

func TestOllamaModelResponds(t *testing.T) {
	ok := httptest.NewServer(ollamaMux(http.StatusOK, nil))
	defer ok.Close()
	t.Setenv("OLLAMA_HOST", ok.URL)
	if !ollamaModelResponds("m", 2*time.Second) {
		t.Error("a 200 model must be reported as responding")
	}
	bad := httptest.NewServer(ollamaMux(http.StatusInternalServerError, nil))
	defer bad.Close()
	t.Setenv("OLLAMA_HOST", bad.URL)
	if ollamaModelResponds("m", 2*time.Second) {
		t.Error("a 500 model must be reported as NOT responding")
	}
	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		time.Sleep(300 * time.Millisecond)
		w.WriteHeader(http.StatusOK)
	}))
	defer slow.Close()
	t.Setenv("OLLAMA_HOST", slow.URL)
	if ollamaModelResponds("m", 50*time.Millisecond) {
		t.Error("a model slower than the deadline must be reported as NOT responding (the hang case)")
	}
}

// TestOllamaFallbackIgnoresDerived → REQ-OC-032: aegis's own aegis-<model> derivatives (which Ollama
// sorts first) must not be chosen as the base — the base must be a real user model.
func TestOllamaFallbackIgnoresDerived(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"aegis-stale:latest"},{"name":"gemma:7b"}]}`)) // aegis-* first
		case "/api/create":
			_, _ = w.Write([]byte(`{"status":"success"}`))
		case "/v1/chat/completions":
			w.WriteHeader(http.StatusOK)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("OLLAMA_HOST", srv.URL)
	oc, models, ok := ollamaFallback(config.Default())
	if !ok || len(models) != 1 || models[0] != "gemma:7b" {
		t.Errorf("the base must be the real user model, not aegis's derivative: models=%v", models)
	}
	if oc.ModelID != "aegis-gemma-7b" {
		t.Errorf("must derive from the real base: model=%q", oc.ModelID)
	}
}

// TestOllamaUsableCandidate -> REQ-OC-043: iterate past a crashing Ollama model to the first working
// one (and mark the crasher), so the screen can offer a real use-now model.
func TestOllamaUsableCandidate(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/tags":
			_, _ = w.Write([]byte(`{"models":[{"name":"bad:1"},{"name":"good:1"}]}`))
		case "/v1/chat/completions":
			var req struct {
				Model string `json:"model"`
			}
			_ = json.NewDecoder(r.Body).Decode(&req)
			if req.Model == "bad:1" {
				w.WriteHeader(http.StatusInternalServerError)
			} else {
				w.WriteHeader(http.StatusOK)
			}
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	t.Setenv("HOME", t.TempDir())
	t.Setenv("OLLAMA_HOST", srv.URL)
	if got := usableOllamaCandidate(); got != "good:1" {
		t.Errorf("must iterate past the crasher to the working model, got %q", got)
	}
	if !ollamaModelUnusable("bad:1") {
		t.Error("the skipped crasher must be marked unusable")
	}
}
