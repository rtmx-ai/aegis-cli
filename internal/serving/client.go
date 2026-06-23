// This file adds the real inference Client to the serving package: an
// OpenAI-compatible HTTP client over the loopback model endpoint. It sits
// alongside serving.go (calibration / launch / Health) and reuses the same
// loopback invariant — the only network call aegis-cli makes is loopback to the
// local serving endpoint, and a non-loopback host is rejected at construction
// (airgap-hygiene, GUARD-001). Standard library only.

package serving

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Message is one OpenAI chat message.
type Message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

// ChatRequest is an OpenAI-compatible chat-completion request. Only a minimal,
// loop-relevant subset of fields is modeled.
type ChatRequest struct {
	// Model is the served model id.
	Model string `json:"model"`
	// Messages is the conversation so far.
	Messages []Message `json:"messages"`
	// Stream requests SSE streaming. ChatCompletion forces this false;
	// ChatCompletionStream forces it true.
	Stream bool `json:"stream,omitempty"`
	// Temperature is the sampling temperature; omitted when zero.
	Temperature float64 `json:"temperature,omitempty"`
	// MaxTokens caps generated tokens; omitted when zero.
	MaxTokens int `json:"max_tokens,omitempty"`
}

// Choice is one non-streaming completion choice.
type Choice struct {
	Index        int     `json:"index"`
	Message      Message `json:"message"`
	FinishReason string  `json:"finish_reason"`
}

// ChatResponse is an OpenAI-compatible non-streaming chat completion.
type ChatResponse struct {
	ID      string   `json:"id"`
	Object  string   `json:"object"`
	Created int64    `json:"created"`
	Model   string   `json:"model"`
	Choices []Choice `json:"choices"`
}

// ChatChunk is one SSE streaming delta.
type ChatChunk struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	Model   string `json:"model"`
	Choices []struct {
		Index int `json:"index"`
		Delta struct {
			Role    string `json:"role"`
			Content string `json:"content"`
		} `json:"delta"`
		FinishReason string `json:"finish_reason"`
	} `json:"choices"`
}

// ModelInfo identifies the served model, for a quant/digest check.
type ModelInfo struct {
	ID     string `json:"id"`
	Digest string `json:"digest"`
}

// APIError is returned for a non-2xx response from the endpoint. It carries the
// status code and the (possibly truncated) response body for diagnosis.
type APIError struct {
	StatusCode int
	Status     string
	Body       string
}

// Error implements error.
func (e *APIError) Error() string {
	return fmt.Sprintf("serving: endpoint returned %d (%s): %s", e.StatusCode, e.Status, e.Body)
}

// Client is an OpenAI-compatible inference client bound to a loopback endpoint.
type Client struct {
	base string
	hc   *http.Client
}

// ClientOption configures a Client.
type ClientOption func(*Client)

// WithHTTPClient sets the HTTP client used for requests. A nil client is
// ignored.
func WithHTTPClient(hc *http.Client) ClientOption {
	return func(c *Client) {
		if hc != nil {
			c.hc = hc
		}
	}
}

// WithTimeout sets the per-request timeout on the client's HTTP client.
func WithTimeout(d time.Duration) ClientOption {
	return func(c *Client) {
		c.hc.Timeout = d
	}
}

// NewClient builds a Client for endpoint, which MUST be a loopback URL.
// A non-loopback host is rejected (egress is forbidden), mirroring the config
// and Health loopback guard.
func NewClient(endpoint string, opts ...ClientOption) (*Client, error) {
	u, err := url.Parse(endpoint)
	if err != nil {
		return nil, fmt.Errorf("serving: invalid endpoint %q: %w", endpoint, err)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return nil, fmt.Errorf("serving: endpoint scheme must be http(s): %q", endpoint)
	}
	if !isLoopbackHost(u.Hostname()) {
		return nil, fmt.Errorf("serving: endpoint %q is not loopback (egress forbidden)", endpoint)
	}
	c := &Client{
		base: strings.TrimRight(u.String(), "/"),
		hc:   &http.Client{Timeout: 60 * time.Second},
	}
	for _, opt := range opts {
		opt(c)
	}
	return c, nil
}

// ChatCompletion performs a non-streaming chat completion.
func (c *Client) ChatCompletion(ctx context.Context, req ChatRequest) (ChatResponse, error) {
	req.Stream = false
	resp, err := c.post(ctx, "/v1/chat/completions", req)
	if err != nil {
		return ChatResponse{}, err
	}
	defer resp.Body.Close()
	if err := checkStatus(resp); err != nil {
		return ChatResponse{}, err
	}
	var out ChatResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return ChatResponse{}, fmt.Errorf("serving: decode chat completion: %w", err)
	}
	return out, nil
}

// ChatCompletionStream performs a streaming chat completion, invoking onChunk
// for each SSE delta in order. An error returned by onChunk aborts the stream
// and is propagated. Context cancellation stops the stream promptly.
func (c *Client) ChatCompletionStream(ctx context.Context, req ChatRequest, onChunk func(ChatChunk) error) error {
	req.Stream = true
	resp, err := c.post(ctx, "/v1/chat/completions", req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if err := checkStatus(resp); err != nil {
		return err
	}
	scanner := bufio.NewScanner(resp.Body)
	// Allow long SSE lines (a full chunk's JSON may be large).
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}
		line := scanner.Text()
		if !strings.HasPrefix(line, "data:") {
			continue
		}
		data := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
		if data == "" {
			continue
		}
		if data == "[DONE]" {
			return nil
		}
		var chunk ChatChunk
		if err := json.Unmarshal([]byte(data), &chunk); err != nil {
			return fmt.Errorf("serving: decode stream chunk: %w", err)
		}
		if err := onChunk(chunk); err != nil {
			return err
		}
	}
	if err := scanner.Err(); err != nil {
		// Surface context cancellation as ctx.Err for callers that select on it.
		if ctxErr := ctx.Err(); ctxErr != nil {
			return ctxErr
		}
		return fmt.Errorf("serving: read stream: %w", err)
	}
	return nil
}

// ModelInfo fetches the served model id and digest from GET /v1/models.
func (c *Client) ModelInfo(ctx context.Context) (ModelInfo, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.base+"/v1/models", nil)
	if err != nil {
		return ModelInfo{}, fmt.Errorf("serving: build models request: %w", err)
	}
	resp, err := c.hc.Do(req)
	if err != nil {
		return ModelInfo{}, fmt.Errorf("serving: models request failed: %w", err)
	}
	defer resp.Body.Close()
	if err := checkStatus(resp); err != nil {
		return ModelInfo{}, err
	}
	var out struct {
		Data []ModelInfo `json:"data"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return ModelInfo{}, fmt.Errorf("serving: decode models: %w", err)
	}
	if len(out.Data) == 0 {
		return ModelInfo{}, fmt.Errorf("serving: models list is empty")
	}
	return out.Data[0], nil
}

// post marshals body and POSTs it as JSON to path under the base endpoint.
func (c *Client) post(ctx context.Context, path string, body any) (*http.Response, error) {
	data, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("serving: marshal request: %w", err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.base+path, bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("serving: build request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	resp, err := c.hc.Do(req)
	if err != nil {
		return nil, fmt.Errorf("serving: request to %s failed: %w", path, err)
	}
	return resp, nil
}

// checkStatus turns a non-2xx response into a typed *APIError, draining the
// body. On success it leaves the body intact for the caller to decode.
func checkStatus(resp *http.Response) error {
	if resp.StatusCode >= 200 && resp.StatusCode < 300 {
		return nil
	}
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 64*1024))
	return &APIError{
		StatusCode: resp.StatusCode,
		Status:     resp.Status,
		Body:       strings.TrimSpace(string(body)),
	}
}
