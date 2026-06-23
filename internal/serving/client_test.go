package serving

import (
	"context"
	"errors"
	"net/http"
	"testing"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
)

// TestChatCompletion → SERVE: client performs a non-streaming chat completion
// against the loopback endpoint; the request shape is sent and the response is
// parsed.
func TestChatCompletion(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{
		Responses: []mockmodel.Response{{Content: "a unified diff"}},
	})
	defer srv.Close()

	c, err := NewClient(srv.URL())
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	resp, err := c.ChatCompletion(context.Background(), ChatRequest{
		Model:       "gemma",
		Messages:    []Message{{Role: "user", Content: "write a diff"}},
		Temperature: 0.2,
	})
	if err != nil {
		t.Fatalf("ChatCompletion: %v", err)
	}
	if len(resp.Choices) != 1 || resp.Choices[0].Message.Content != "a unified diff" {
		t.Fatalf("unexpected response: %+v", resp)
	}

	last, ok := srv.LastRequest()
	if !ok {
		t.Fatal("no request recorded")
	}
	if last.Model != "gemma" {
		t.Errorf("recorded model = %q, want gemma", last.Model)
	}
	if last.Stream {
		t.Error("non-streaming request recorded as stream=true")
	}
	if len(last.Messages) != 1 || last.Messages[0].Content != "write a diff" {
		t.Errorf("recorded messages = %+v", last.Messages)
	}
	if last.Temperature != 0.2 {
		t.Errorf("recorded temperature = %v, want 0.2", last.Temperature)
	}
}

// TestChatCompletionStreamOrder → SERVE: streaming completion delivers SSE
// chunks to the callback in order.
func TestChatCompletionStreamOrder(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{
		Responses: []mockmodel.Response{{Content: "one two three", ChunkWords: true}},
	})
	defer srv.Close()

	c, err := NewClient(srv.URL())
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	var got string
	err = c.ChatCompletionStream(context.Background(), ChatRequest{
		Model:    "gemma",
		Messages: []Message{{Role: "user", Content: "count"}},
	}, func(ch ChatChunk) error {
		for _, choice := range ch.Choices {
			got += choice.Delta.Content
		}
		return nil
	})
	if err != nil {
		t.Fatalf("ChatCompletionStream: %v", err)
	}
	if want := "one two three "; got != want {
		t.Fatalf("assembled stream = %q, want %q", got, want)
	}

	last, ok := srv.LastRequest()
	if !ok || !last.Stream {
		t.Fatalf("stream request not recorded as stream=true: %+v ok=%v", last, ok)
	}
}

// TestChatCompletionStreamCallbackError → SERVE: an error from the streaming
// callback aborts the stream and propagates.
func TestChatCompletionStreamCallbackError(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{
		Responses: []mockmodel.Response{{Content: "alpha beta gamma", ChunkWords: true}},
	})
	defer srv.Close()

	c, _ := NewClient(srv.URL())

	sentinel := errors.New("stop please")
	calls := 0
	err := c.ChatCompletionStream(context.Background(), ChatRequest{
		Model:    "gemma",
		Messages: []Message{{Role: "user", Content: "x"}},
	}, func(ch ChatChunk) error {
		calls++
		return sentinel
	})
	if !errors.Is(err, sentinel) {
		t.Fatalf("err = %v, want sentinel", err)
	}
	if calls != 1 {
		t.Fatalf("callback called %d times, want 1 (aborted on first error)", calls)
	}
}

// TestChatCompletionStreamCtxCancel → SERVE: canceling the context stops a
// streaming completion.
func TestChatCompletionStreamCtxCancel(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{
		Responses: []mockmodel.Response{{Content: "a b c d e f g", ChunkWords: true}},
	})
	defer srv.Close()

	c, _ := NewClient(srv.URL())

	ctx, cancel := context.WithCancel(context.Background())
	err := c.ChatCompletionStream(ctx, ChatRequest{
		Model:    "gemma",
		Messages: []Message{{Role: "user", Content: "x"}},
	}, func(ch ChatChunk) error {
		cancel() // cancel after the first chunk
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled", err)
	}
}

// TestChatCompletionNon2xx → SERVE: a non-2xx endpoint response yields a typed
// APIError carrying status and body.
func TestChatCompletionNon2xx(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{
		Responses: []mockmodel.Response{{Status: http.StatusInternalServerError, Body: "model exploded"}},
	})
	defer srv.Close()

	c, _ := NewClient(srv.URL())
	_, err := c.ChatCompletion(context.Background(), ChatRequest{
		Model:    "gemma",
		Messages: []Message{{Role: "user", Content: "x"}},
	})
	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("err = %v, want *APIError", err)
	}
	if apiErr.StatusCode != http.StatusInternalServerError {
		t.Errorf("status = %d, want 500", apiErr.StatusCode)
	}
	if apiErr.Body != "model exploded" {
		t.Errorf("body = %q, want %q", apiErr.Body, "model exploded")
	}
}

// TestNewClientRejectsNonLoopback → GUARD: a non-loopback endpoint is rejected
// at construction (egress forbidden).
func TestNewClientRejectsNonLoopback(t *testing.T) {
	for _, ep := range []string{
		"http://example.com:8080",
		"http://10.0.0.5:8080",
		"https://api.openai.com",
	} {
		if _, err := NewClient(ep); err == nil {
			t.Errorf("NewClient(%q) = nil error, want rejection", ep)
		}
	}
	// Loopback forms are accepted.
	for _, ep := range []string{
		"http://127.0.0.1:8080",
		"http://localhost:8080",
		"http://[::1]:8080",
	} {
		if _, err := NewClient(ep); err != nil {
			t.Errorf("NewClient(%q) = %v, want accepted", ep, err)
		}
	}
}

// TestChatCompletionCtxTimeout → SERVE: a context deadline is honored and stops
// a request.
func TestChatCompletionCtxTimeout(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{})
	defer srv.Close()

	c, _ := NewClient(srv.URL())

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // already canceled before the call

	_, err := c.ChatCompletion(ctx, ChatRequest{
		Model:    "gemma",
		Messages: []Message{{Role: "user", Content: "x"}},
	})
	if err == nil {
		t.Fatal("expected error from canceled context")
	}
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled wrapped", err)
	}
}

// TestModelInfo → SERVE: client reads the served model id + digest for a
// quant/digest check.
func TestModelInfo(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{
		ModelID:     "gemma-4-26b",
		ModelDigest: "sha256:deadbeef",
	})
	defer srv.Close()

	c, _ := NewClient(srv.URL(), WithTimeout(5*time.Second))
	info, err := c.ModelInfo(context.Background())
	if err != nil {
		t.Fatalf("ModelInfo: %v", err)
	}
	if info.ID != "gemma-4-26b" {
		t.Errorf("id = %q, want gemma-4-26b", info.ID)
	}
	if info.Digest != "sha256:deadbeef" {
		t.Errorf("digest = %q, want sha256:deadbeef", info.Digest)
	}
}

// TestPreflightSmoke models SERVE-012: a minimal completion succeeds against a
// live endpoint, and a dead endpoint yields a timely error.
func TestPreflightSmoke(t *testing.T) {
	mock := mockmodel.New(mockmodel.Options{})
	c, err := NewClient(mock.URL())
	if err != nil {
		t.Fatal(err)
	}
	if err := c.PreflightSmoke(context.Background()); err != nil {
		t.Errorf("preflight smoke against a live endpoint must pass: %v", err)
	}
	mock.Close() // endpoint now dead
	if err := c.PreflightSmoke(context.Background()); err == nil {
		t.Error("preflight smoke against a dead endpoint must fail")
	}
}

// TestModelDigestGate models SERVE-013: the served model id+digest is checked
// against expected values; a mismatch errors, a match (and empty/skip) passes.
func TestModelDigestGate(t *testing.T) {
	mock := mockmodel.New(mockmodel.Options{ModelID: "gemma-4-26b", ModelDigest: "sha256:abc123"})
	defer mock.Close()
	c, err := NewClient(mock.URL())
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	if err := c.CheckModel(ctx, "gemma-4-26b", "sha256:abc123"); err != nil {
		t.Errorf("matching id+digest must pass: %v", err)
	}
	if err := c.CheckModel(ctx, "", ""); err != nil {
		t.Errorf("empty expectations must skip the gate: %v", err)
	}
	if err := c.CheckModel(ctx, "gemma-4-26b", "sha256:WRONG"); err == nil {
		t.Error("a digest mismatch must error")
	}
	if err := c.CheckModel(ctx, "qwen-wrong", ""); err == nil {
		t.Error("an id mismatch must error")
	}
}

// TestClientReportsTiming models SERVE-014: a completion surfaces token counts
// and a measured latency for the metrics dashboard.
func TestClientReportsTiming(t *testing.T) {
	mock := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{{Content: "one two three"}}})
	defer mock.Close()
	c, err := NewClient(mock.URL())
	if err != nil {
		t.Fatal(err)
	}
	resp, err := c.ChatCompletion(context.Background(), ChatRequest{Model: "m", Messages: []Message{{Role: "user", Content: "hi"}}})
	if err != nil {
		t.Fatal(err)
	}
	if resp.Usage.TotalTokens <= 0 || resp.Usage.CompletionTokens <= 0 {
		t.Errorf("expected token counts, got %+v", resp.Usage)
	}
	if resp.Latency < 0 {
		t.Errorf("expected a measured latency, got %v", resp.Latency)
	}
}
