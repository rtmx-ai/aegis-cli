package main

import (
	"encoding/json"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

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
// Ollama is not running. OC-028: so an operator who already runs Ollama uses those models instead of
// being told there are none. (Probe only — connecting to a local Ollama the operator already runs is
// loopback, not egress.)
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

// ollamaFallback points cfg at a running Ollama (its OpenAI-compatible endpoint + first installed
// model) when no local model is provisioned but Ollama is up. Returns (cfg, models, true) on a hit,
// (cfg, nil, false) otherwise. OC-028.
func ollamaFallback(cfg config.Config) (config.Config, []string, bool) {
	models := detectOllama()
	if len(models) == 0 {
		return cfg, nil, false
	}
	cfg.Endpoint = ollamaHost()
	cfg.ModelID = models[0]
	return cfg, models, true
}
