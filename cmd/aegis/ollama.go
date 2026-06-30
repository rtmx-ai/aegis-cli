package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// ollamaCtxTokens is the context aegis bakes into its derived Ollama model (OC-029). opencode's agent
// prompt (system + tool schemas) overflows Ollama's small default num_ctx, so without this the prompt
// is truncated and the model flails. 16k fits the prompt + a working conversation.
const ollamaCtxTokens = 16384

// ollamaHost returns the Ollama base URL — OLLAMA_HOST or the default localhost:11434, normalized.
func ollamaHost() string {
	h := os.Getenv("OLLAMA_HOST")
	if h == "" {
		return "http://localhost:11434"
	}
	if !strings.HasPrefix(h, "http://") && !strings.HasPrefix(h, "https://") {
		h = "http://" + h
	}
	return strings.TrimRight(h, "/")
}

// detectOllama probes a running Ollama's tag list and returns its installed model names, or nil when
// Ollama is not running. OC-028.
func detectOllama() []string {
	client := &http.Client{Timeout: 2 * time.Second}
	resp, err := client.Get(ollamaHost() + "/api/tags") //nolint:gosec // operator's local Ollama, loopback
	if err != nil {
		return nil
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil
	}
	var tags struct {
		Models []struct {
			Name string `json:"name"`
		} `json:"models"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&tags); err != nil {
		return nil
	}
	var names []string
	for _, m := range tags.Models {
		if m.Name != "" {
			names = append(names, m.Name)
		}
	}
	return names
}

func ollamaDerivedName(base string) string {
	return "aegis-" + strings.NewReplacer(":", "-", "/", "-").Replace(base)
}

// ensureOllamaCtxModel creates a lightweight derived model (aegis-<model>) with num_ctx baked in, so
// opencode's large agent prompt is not truncated — the OpenAI-compat /v1 endpoint cannot override
// num_ctx per request. Returns the derived name, or the base unchanged on failure or when a prior
// launch found the derived model hangs this base's load (OC-031). Idempotent.
func ensureOllamaCtxModel(base string) string {
	if ollamaCtxBroken(base) {
		return base // OC-031: a prior launch found the derived ctx model hangs this model's load
	}
	derived := ollamaDerivedName(base)
	for _, m := range detectOllama() {
		if m == derived || m == derived+":latest" {
			return derived
		}
	}
	body, err := json.Marshal(map[string]any{
		"model": derived, "from": base,
		"parameters": map[string]any{"num_ctx": ollamaCtxTokens}, "stream": false,
	})
	if err != nil {
		return base
	}
	client := &http.Client{Timeout: 60 * time.Second}
	resp, err := client.Post(ollamaHost()+"/api/create", "application/json", bytes.NewReader(body)) //nolint:gosec
	if err != nil {
		return base
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, resp.Body)
	if resp.StatusCode != http.StatusOK {
		return base
	}
	return derived
}

// ollamaModelResponds probes a model with a 1-token generation under a deadline — true if it loads +
// answers, false if it hangs/errors. OC-031: a derived num_ctx model can hang Ollama's Metal load
// (Gemma 3n), so aegis verifies it works before handing it to opencode.
func ollamaModelResponds(model string, timeout time.Duration) bool {
	body, err := json.Marshal(map[string]any{
		"model":      model,
		"messages":   []map[string]string{{"role": "user", "content": "hi"}},
		"max_tokens": 1, "stream": false,
	})
	if err != nil {
		return false
	}
	client := &http.Client{Timeout: timeout}
	resp, err := client.Post(ollamaHost()+"/v1/chat/completions", "application/json", bytes.NewReader(body)) //nolint:gosec
	if err != nil {
		return false
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, resp.Body)
	return resp.StatusCode == http.StatusOK
}

func removeOllamaModel(model string) {
	body, err := json.Marshal(map[string]string{"model": model})
	if err != nil {
		return
	}
	req, err := http.NewRequest(http.MethodDelete, ollamaHost()+"/api/delete", bytes.NewReader(body))
	if err != nil {
		return
	}
	req.Header.Set("content-type", "application/json")
	if resp, derr := (&http.Client{Timeout: 10 * time.Second}).Do(req); derr == nil { //nolint:gosec
		_ = resp.Body.Close()
	}
}

// ollamaSkipCtxMarker / ollamaCtxBroken / markOllamaCtxBroken remember that a base model's derived
// ctx variant hangs its load, so aegis does not recreate + re-probe it every launch (OC-031).
func ollamaSkipCtxMarker(base string) string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".config", "aegis", "ollama-no-ctx-"+strings.NewReplacer(":", "-", "/", "-").Replace(base))
}
func ollamaCtxBroken(base string) bool {
	m := ollamaSkipCtxMarker(base)
	if m == "" {
		return false
	}
	_, err := os.Stat(m)
	return err == nil
}
func markOllamaCtxBroken(base string) {
	if m := ollamaSkipCtxMarker(base); m != "" {
		_ = os.MkdirAll(filepath.Dir(m), 0o755)
		_ = os.WriteFile(m, []byte("derived ctx model hangs this model's Ollama load; using the base\n"), 0o644)
	}
}

// ollamaUnusableMarker / ollamaModelUnusable / markOllamaModelUnusable remember that a base model
// crashes the backend on generation (Gemma 3n: GGML_SCHED_MAX_SPLIT_INPUTS), so aegis skips it on the
// next launch instead of re-probing it for 30s (OC-036).
func ollamaUnusableMarker(model string) string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".config", "aegis", "ollama-unusable-"+strings.NewReplacer(":", "-", "/", "-").Replace(model))
}
func ollamaModelUnusable(model string) bool {
	m := ollamaUnusableMarker(model)
	if m == "" {
		return false
	}
	_, err := os.Stat(m)
	return err == nil
}
func markOllamaModelUnusable(model string) {
	if m := ollamaUnusableMarker(model); m != "" {
		_ = os.MkdirAll(filepath.Dir(m), 0o755)
		_ = os.WriteFile(m, []byte("this Ollama model crashes the backend on generation; aegis skips it\n"), 0o644)
	}
}

// ollamaFallback points cfg at a running Ollama (its OpenAI-compatible endpoint + a usable model)
// when no local model is provisioned but Ollama is up. OC-028/029/031: prefers a num_ctx-sized
// derived model, but only after verifying it actually loads + answers — else it drops the derived
// model and uses the base, never handing opencode a model that hangs.
func ollamaFallback(cfg config.Config) (config.Config, []string, bool) {
	var models []string
	for _, m := range detectOllama() {
		if strings.HasPrefix(m, "aegis-") { // OC-032: never treat aegis's own derived models as the base
			continue
		}
		if ollamaModelUnusable(m) { // OC-036: a prior launch found this model crashes generation — skip it
			continue
		}
		models = append(models, m)
	}
	if len(models) == 0 {
		return cfg, nil, false
	}
	cfg.Endpoint = ollamaHost()
	base := models[0]
	model := ensureOllamaCtxModel(base)
	if model != base && !ollamaModelResponds(model, 25*time.Second) {
		markOllamaCtxBroken(base)
		removeOllamaModel(model)
		model = base
	}
	// OC-035: probe the FINAL model we'd hand opencode. A model that loads but crashes the backend on
	// generation (Gemma 3n: GGML_SCHED_MAX_SPLIT_INPUTS -> HTTP 500) would otherwise freeze the TUI on
	// the first prompt. If it can't generate, report no usable Ollama so the caller shows the
	// provisioning screen instead of a silent hang.
	if !ollamaModelResponds(model, 30*time.Second) {
		markOllamaModelUnusable(base) // OC-036: remember so the next launch skips the 30s re-probe
		return cfg, nil, false
	}
	cfg.ModelID = model
	return cfg, models, true
}
