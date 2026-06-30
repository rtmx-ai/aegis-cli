package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// ollamaCtxTokens is the context aegis bakes into its derived Ollama model (OC-029). opencode's agent
// prompt (system + tool schemas) overflows Ollama's small default num_ctx, so without this the prompt
// is truncated and the model flails. 16k fits the prompt + a working conversation.
const ollamaCtxTokens = 16384

// ollamaHost returns the Ollama base URL — OLLAMA_HOST (the standard Ollama env) or the default
// localhost:11434. Always normalized to a scheme-qualified, trailing-slash-free URL.
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
// Ollama is not running. OC-028. (Probe only — connecting to a local Ollama the operator already runs
// is loopback, not egress.)
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

// ensureOllamaCtxModel makes a usable-context variant of the Ollama model: a lightweight derived model
// (aegis-<model>, sharing the base weights) with num_ctx baked in, so opencode's large agent prompt is
// not truncated by Ollama's small default context — which the OpenAI-compatible /v1 endpoint cannot
// override per request (verified). Returns the derived model name, or the base unchanged on any
// failure (degrade, don't block). Idempotent: skips the create when the derived model already exists.
func ensureOllamaCtxModel(base string) string {
	derived := "aegis-" + strings.NewReplacer(":", "-", "/", "-").Replace(base)
	for _, m := range detectOllama() {
		if m == derived || m == derived+":latest" {
			return derived
		}
	}
	body, err := json.Marshal(map[string]any{
		"model":      derived,
		"from":       base,
		"parameters": map[string]any{"num_ctx": ollamaCtxTokens},
		"stream":     false,
	})
	if err != nil {
		return base
	}
	client := &http.Client{Timeout: 60 * time.Second}
	resp, err := client.Post(ollamaHost()+"/api/create", "application/json", bytes.NewReader(body)) //nolint:gosec // local Ollama
	if err != nil {
		return base
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, resp.Body) // drain the streamed status lines
	if resp.StatusCode != http.StatusOK {
		return base
	}
	return derived
}

// ollamaFallback points cfg at a running Ollama (its OpenAI-compatible endpoint + a context-sized
// variant of its first model) when no local model is provisioned but Ollama is up. Returns
// (cfg, models, true) on a hit, (cfg, nil, false) otherwise. OC-028 + OC-029.
func ollamaFallback(cfg config.Config) (config.Config, []string, bool) {
	models := detectOllama()
	if len(models) == 0 {
		return cfg, nil, false
	}
	cfg.Endpoint = ollamaHost()
	cfg.ModelID = ensureOllamaCtxModel(models[0]) // OC-029: a num_ctx-sized variant so the agent prompt fits
	return cfg, models, true
}
