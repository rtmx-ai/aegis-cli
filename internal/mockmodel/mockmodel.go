// Package mockmodel is a programmable, loopback-bound, OpenAI-compatible mock
// inference server for tests and end-to-end runs. It lets the whole serving
// stack be exercised in CI without a real GGUF: a test queues canned chat
// completions (or supplies a handler), runs the loop against URL(), and then
// asserts on the requests the server recorded.
//
// It is standard-library-only (net/http/httptest) and binds to 127.0.0.1 on an
// ephemeral port, preserving the loopback-only / zero-egress invariant. It is a
// test helper: it ships in no production code path and links no third-party
// dependency.
package mockmodel

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"time"
)

// Message is one OpenAI chat message.
type Message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

// RecordedRequest captures the salient fields of a received chat-completion
// request so a test can assert on what the client actually sent.
type RecordedRequest struct {
	// Path is the request URL path (e.g. "/v1/chat/completions").
	Path string
	// Method is the HTTP method.
	Method string
	// Model is the requested model id.
	Model string
	// Messages are the chat messages sent by the client.
	Messages []Message
	// Stream is the request's stream flag.
	Stream bool
	// Temperature is the requested sampling temperature.
	Temperature float64
	// RawBody is the verbatim request body.
	RawBody []byte
}

// Response is a single scripted chat-completion reply. Content is returned as
// the assistant message; when the request asks for streaming it is split into
// SSE chunks (see ChunkWords).
type Response struct {
	// Content is the assistant reply text.
	Content string
	// FinishReason is the choice finish_reason; defaults to "stop".
	FinishReason string
	// Status, when non-zero, overrides the HTTP status code so a test can
	// drive the non-2xx error path. The Body field is then returned verbatim.
	Status int
	// Body, when Status is set, is the raw error body returned to the client.
	Body string
	// ChunkWords, when true, streams the content word-by-word (one SSE chunk
	// per whitespace-separated token) instead of a single content chunk. It
	// only affects streaming requests.
	ChunkWords bool
}

// Options configures a mock server.
type Options struct {
	// Responses is the queue of scripted replies, consumed in order, one per
	// chat-completion request. When the queue is empty a default reply is
	// returned (or Handler is consulted, if set).
	Responses []Response
	// Handler, when set, is consulted for each chat-completion request before
	// the queue. Returning ok=false falls through to the queue/default.
	Handler func(RecordedRequest) (resp Response, ok bool)
	// ModelID is the id reported by GET /v1/models. Defaults to "mock-model".
	ModelID string
	// ModelDigest is the digest reported by GET /v1/models. Defaults to
	// "sha256:0000".
	ModelDigest string
}

// Server is a running mock model server.
type Server struct {
	ts          *httptest.Server
	modelID     string
	modelDigest string
	handler     func(RecordedRequest) (Response, bool)

	mu       sync.Mutex
	queue    []Response
	requests []RecordedRequest
}

// New starts a loopback-bound mock model server. Call Close when done.
func New(opts Options) *Server {
	s := &Server{
		modelID:     orDefault(opts.ModelID, "mock-model"),
		modelDigest: orDefault(opts.ModelDigest, "sha256:0000"),
		handler:     opts.Handler,
		queue:       append([]Response(nil), opts.Responses...),
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/health", s.handleHealth)
	mux.HandleFunc("/v1/models", s.handleModels)
	mux.HandleFunc("/v1/chat/completions", s.handleChat)
	// httptest.NewServer binds to 127.0.0.1 on an ephemeral port.
	s.ts = httptest.NewServer(mux)
	return s
}

// URL returns the loopback base URL the server is listening on (no trailing
// slash), e.g. "http://127.0.0.1:54321".
func (s *Server) URL() string { return s.ts.URL }

// Close shuts the server down.
func (s *Server) Close() { s.ts.Close() }

// Enqueue appends scripted responses to the reply queue.
func (s *Server) Enqueue(resps ...Response) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.queue = append(s.queue, resps...)
}

// Requests returns a copy of every chat-completion request received so far, in
// order.
func (s *Server) Requests() []RecordedRequest {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]RecordedRequest(nil), s.requests...)
}

// LastRequest returns the most recent chat-completion request and whether one
// has been received.
func (s *Server) LastRequest() (RecordedRequest, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.requests) == 0 {
		return RecordedRequest{}, false
	}
	return s.requests[len(s.requests)-1], true
}

func (s *Server) handleHealth(w http.ResponseWriter, _ *http.Request) {
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write([]byte("ok"))
}

func (s *Server) handleModels(w http.ResponseWriter, _ *http.Request) {
	out := map[string]any{
		"object": "list",
		"data": []map[string]any{
			{
				"id":       s.modelID,
				"object":   "model",
				"digest":   s.modelDigest,
				"created":  time.Now().Unix(),
				"owned_by": "mockmodel",
			},
		},
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(out)
}

// chatRequest is the subset of the OpenAI chat-completion request we decode.
type chatRequest struct {
	Model       string    `json:"model"`
	Messages    []Message `json:"messages"`
	Stream      bool      `json:"stream"`
	Temperature float64   `json:"temperature"`
}

func (s *Server) handleChat(w http.ResponseWriter, r *http.Request) {
	body := readAll(r)
	var cr chatRequest
	_ = json.Unmarshal(body, &cr)
	rec := RecordedRequest{
		Path:        r.URL.Path,
		Method:      r.Method,
		Model:       cr.Model,
		Messages:    cr.Messages,
		Stream:      cr.Stream,
		Temperature: cr.Temperature,
		RawBody:     body,
	}
	resp := s.next(rec)

	// Programmable error path.
	if resp.Status != 0 && resp.Status/100 != 2 {
		http.Error(w, resp.Body, resp.Status)
		return
	}
	finish := orDefault(resp.FinishReason, "stop")

	if cr.Stream {
		s.streamCompletion(w, cr.Model, resp, finish)
		return
	}
	s.jsonCompletion(w, cr.Model, resp, finish)
}

// next records the request and resolves the scripted response: Handler first
// (if it claims the request), then the queue, then a default echo reply.
func (s *Server) next(rec RecordedRequest) Response {
	s.mu.Lock()
	s.requests = append(s.requests, rec)
	s.mu.Unlock()

	if s.handler != nil {
		if resp, ok := s.handler(rec); ok {
			return resp
		}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if len(s.queue) > 0 {
		resp := s.queue[0]
		s.queue = s.queue[1:]
		return resp
	}
	return Response{Content: "ok"}
}

func (s *Server) jsonCompletion(w http.ResponseWriter, model string, resp Response, finish string) {
	out := map[string]any{
		"id":      "chatcmpl-mock",
		"object":  "chat.completion",
		"created": time.Now().Unix(),
		"model":   model,
		"choices": []map[string]any{
			{
				"index":         0,
				"finish_reason": finish,
				"message": map[string]any{
					"role":    "assistant",
					"content": resp.Content,
				},
			},
		},
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(out)
}

func (s *Server) streamCompletion(w http.ResponseWriter, model string, resp Response, finish string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)

	emit := func(content, finishReason string) {
		delta := map[string]any{}
		if content != "" {
			delta["content"] = content
		}
		choice := map[string]any{"index": 0, "delta": delta}
		if finishReason != "" {
			choice["finish_reason"] = finishReason
		}
		chunk := map[string]any{
			"id":      "chatcmpl-mock",
			"object":  "chat.completion.chunk",
			"created": time.Now().Unix(),
			"model":   model,
			"choices": []map[string]any{choice},
		}
		b, _ := json.Marshal(chunk)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", b)
		if flusher != nil {
			flusher.Flush()
		}
	}

	// First chunk carries the assistant role (OpenAI convention).
	emit("", "")
	if resp.ChunkWords {
		for _, word := range strings.Fields(resp.Content) {
			emit(word+" ", "")
		}
	} else if resp.Content != "" {
		emit(resp.Content, "")
	}
	emit("", finish)
	_, _ = fmt.Fprint(w, "data: [DONE]\n\n")
	if flusher != nil {
		flusher.Flush()
	}
}

func readAll(r *http.Request) []byte {
	if r.Body == nil {
		return nil
	}
	defer r.Body.Close()
	var sb strings.Builder
	buf := make([]byte, 4096)
	for {
		n, err := r.Body.Read(buf)
		if n > 0 {
			sb.Write(buf[:n])
		}
		if err != nil {
			break
		}
	}
	return []byte(sb.String())
}

func orDefault(v, def string) string {
	if v == "" {
		return def
	}
	return v
}
