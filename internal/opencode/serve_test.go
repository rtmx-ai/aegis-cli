package opencode

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// mockServe stands in for `opencode serve`: readiness + the /api session routes
// aegis drives, recording the prompt it receives.
func mockServe(t *testing.T, gotPrompt *string) *httptest.Server {
	t.Helper()
	mux := http.NewServeMux()
	mux.HandleFunc("/openapi.json", func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte("{}")) })
	mux.HandleFunc("/api/session", func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"data": map[string]any{"id": "ses_1"}})
	})
	mux.HandleFunc("/api/session/ses_1/prompt", func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		*gotPrompt = string(b)
		_ = json.NewEncoder(w).Encode(map[string]any{"data": map[string]any{}})
	})
	mux.HandleFunc("/api/session/ses_1/message", func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"data": []map[string]any{
			{"info": map[string]any{"role": "user"}, "parts": []map[string]any{{"type": "text", "text": "build X"}}},
			{"info": map[string]any{"role": "assistant", "finish": "stop",
				"tokens": map[string]any{"total": 142, "input": 100, "output": 42}},
				"parts": []map[string]any{{"type": "text", "text": "done"}}},
		}})
	})
	ts := httptest.NewServer(mux)
	t.Cleanup(ts.Close)
	return ts
}

// TestServeClientDrive → BENCH-001 (client mechanics): create -> prompt -> collect
// transcript with usage over the /api surface. (The real autonomous run is gated
// on an upstream gap; see serve.go.)
func TestServeClientDrive(t *testing.T) {
	var prompt string
	ts := mockServe(t, &prompt)
	c := NewServeClient(ts.URL)
	c.dir = "/w"
	ctx := context.Background()

	if !c.Ready(ctx) {
		t.Fatal("Ready should be true against a live serve")
	}
	id, err := c.CreateSession(ctx, Model{ProviderID: "local", ModelID: "phi4-mini"})
	if err != nil || id != "ses_1" {
		t.Fatalf("CreateSession: id=%q err=%v", id, err)
	}
	if err := c.Prompt(ctx, id, "build X"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	if !strings.Contains(prompt, "build X") {
		t.Errorf("serve must receive the prompt text; got %s", prompt)
	}
	msgs, err := c.Messages(ctx, id)
	if err != nil || len(msgs) != 2 {
		t.Fatalf("Messages: n=%d err=%v", len(msgs), err)
	}
	a := msgs[1]
	if a.Role != "assistant" || a.Tokens.Total != 142 || a.Text != "done" || a.Finish != "stop" {
		t.Errorf("assistant message not parsed (incl. usage): %+v", a)
	}
}
