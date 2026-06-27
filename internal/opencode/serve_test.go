package opencode

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// mockServe stands in for `opencode serve`: readiness + the real /session routes
// aegis drives (POST /session, POST+GET /session/{id}/message), recording the
// message body it receives. It fails the test if the client touches the wrong v2
// /api/session surface (the queue route that never executes).
func mockServe(t *testing.T, gotBody *string) *httptest.Server {
	t.Helper()
	mux := http.NewServeMux()
	// The real server answers /openapi.json with the web UI (HTML), not JSON.
	mux.HandleFunc("/openapi.json", func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte("<!doctype html><html></html>"))
	})
	// POST /session returns the session object flat (no "data" wrapper).
	mux.HandleFunc("/session", func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"id": "ses_1"})
	})
	mux.HandleFunc("/session/ses_1/message", func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost {
			b, _ := io.ReadAll(r.Body)
			*gotBody = string(b)
			// Synchronous executor returns the assistant message on completion.
			_ = json.NewEncoder(w).Encode(map[string]any{"info": map[string]any{"role": "assistant"}, "parts": []any{}})
			return
		}
		// GET: the full transcript with per-message usage, a flat top-level array.
		_ = json.NewEncoder(w).Encode([]map[string]any{
			{"info": map[string]any{"role": "user"}, "parts": []map[string]any{{"type": "text", "text": "build X"}}},
			{"info": map[string]any{"role": "assistant", "finish": "stop",
				"tokens": map[string]any{"total": 142, "input": 100, "output": 42}},
				"parts": []map[string]any{{"type": "text", "text": "done"}}},
		})
	})
	// The v2 queue surface must NOT be used.
	mux.HandleFunc("/api/session", func(http.ResponseWriter, *http.Request) {
		t.Error("client must drive /session, not the v2 /api/session queue route")
	})
	ts := httptest.NewServer(mux)
	t.Cleanup(ts.Close)
	return ts
}

// TestServeDriveSynchronous → REQ-BENCH-006: aegis drives a turn via the serve
// synchronous executor — create POST /session, drive POST /session/{id}/message
// with {parts,model,agent}, collect GET /session/{id}/message with usage — over
// /session (no /api prefix, no /wait).
func TestServeDriveSynchronous(t *testing.T) {
	var body string
	ts := mockServe(t, &body)
	c := NewServeClient(ts.URL)
	c.dir = "/w"
	ctx := context.Background()

	if !c.Ready(ctx) {
		t.Fatal("Ready should be true against a live serve (HTML 200)")
	}
	res, err := c.Drive(ctx, Model{ProviderID: "local", ModelID: "gemma4-qat:32k"}, "build X")
	if err != nil {
		t.Fatalf("Drive: %v", err)
	}
	if res.SessionID != "ses_1" {
		t.Errorf("session id = %q, want ses_1", res.SessionID)
	}
	// The prompt went to the synchronous executor with parts + model + agent.
	for _, want := range []string{`"type":"text"`, `"text":"build X"`, `"modelID":"gemma4-qat:32k"`, `"agent":"build"`} {
		if !strings.Contains(body, want) {
			t.Errorf("message body missing %s; got %s", want, body)
		}
	}
	if len(res.Messages) != 2 {
		t.Fatalf("transcript n=%d, want 2", len(res.Messages))
	}
	a := res.Messages[1]
	if a.Role != "assistant" || a.Tokens.Total != 142 || a.Text != "done" || a.Finish != "stop" {
		t.Errorf("assistant message not parsed (incl. usage): %+v", a)
	}
}

// TestServeDriveBudgetPartial → REQ-BENCH-008 (RUNQ-001 preserved on the serve
// path): when the run's wall-clock budget expires mid-turn, Drive does not error —
// it returns the partial transcript with TimedOut set.
func TestServeDriveBudgetPartial(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/session", func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"id": "ses_b"})
	})
	mux.HandleFunc("/session/ses_b/message", func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost {
			// Don't answer before the run's (much shorter) budget expires, so
			// SendMessage times out; then return promptly so Close doesn't stall.
			time.Sleep(300 * time.Millisecond)
			return
		}
		// GET: the partial transcript collected so far.
		_ = json.NewEncoder(w).Encode([]map[string]any{
			{"info": map[string]any{"role": "user"}, "parts": []map[string]any{{"type": "text", "text": "x"}}},
			{"info": map[string]any{"role": "assistant", "tokens": map[string]any{"total": 9}},
				"parts": []map[string]any{{"type": "text", "text": "partial"}}},
		})
	})
	ts := httptest.NewServer(mux)
	defer ts.Close()
	c := NewServeClient(ts.URL)
	c.dir = "/w"

	ctx, cancel := context.WithTimeout(context.Background(), 150*time.Millisecond)
	defer cancel()
	res, err := c.Drive(ctx, Model{ProviderID: "local", ModelID: "m"}, "x")
	if err != nil {
		t.Fatalf("budget expiry must not be a hard error: %v", err)
	}
	if !res.TimedOut {
		t.Error("expected TimedOut=true on budget expiry")
	}
	if len(res.Messages) == 0 {
		t.Error("expected a partial transcript on budget expiry")
	}
}
