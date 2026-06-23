package mockmodel

import (
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"
)

// TestServerBootsAndScripts → SERVE: the mock model server boots on loopback,
// answers /health 200, and returns a scripted chat completion.
func TestServerBootsAndScripts(t *testing.T) {
	s := New(Options{
		Responses: []Response{{Content: "scripted diff"}},
	})
	defer s.Close()

	if !strings.HasPrefix(s.URL(), "http://127.0.0.1:") {
		t.Fatalf("server not bound to loopback: %s", s.URL())
	}

	// /health 200.
	hresp, err := http.Get(s.URL() + "/health")
	if err != nil {
		t.Fatalf("health: %v", err)
	}
	hresp.Body.Close()
	if hresp.StatusCode != http.StatusOK {
		t.Fatalf("health status = %d, want 200", hresp.StatusCode)
	}

	// Scripted chat completion.
	body := `{"model":"m","messages":[{"role":"user","content":"hi"}]}`
	cresp, err := http.Post(s.URL()+"/v1/chat/completions", "application/json", strings.NewReader(body))
	if err != nil {
		t.Fatalf("chat: %v", err)
	}
	defer cresp.Body.Close()
	raw, _ := io.ReadAll(cresp.Body)
	var out struct {
		Choices []struct {
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
		} `json:"choices"`
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("decode: %v (%s)", err, raw)
	}
	if len(out.Choices) != 1 || out.Choices[0].Message.Content != "scripted diff" {
		t.Fatalf("unexpected completion: %s", raw)
	}

	// Request was recorded.
	if last, ok := s.LastRequest(); !ok || last.Model != "m" {
		t.Fatalf("request not recorded correctly: %+v ok=%v", last, ok)
	}
}
