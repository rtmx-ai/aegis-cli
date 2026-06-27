package opencode

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// This is the headless agent-run surface (BENCH-006): aegis drives OpenCode
// through its `serve` HTTP API rather than the interactive TUI. The real contract,
// reverse-engineered + proven against v1.17.9 (see docs/requirements/
// headless-serve-drive.md):
//   - the session routes live under /session (NOT /api/session — that is the v2
//     surface, whose POST /api/session/{id}/prompt only QUEUES and whose /wait is
//     an empty stub; unmatched routes fall through to the web UI as HTML 200).
//   - readiness: GET /openapi.json answers 200 (HTML web UI) once listening.
//   - server is unsecured by default (no OPENCODE_SERVER_PASSWORD); when a state
//     password exists, auth is HTTP Basic with username "opencode".
//   - flow: POST /session {agent,model,location} -> POST /session/{id}/message
//     {parts,model,agent} (SYNCHRONOUS — blocks until the turn completes) -> GET
//     /session/{id}/message for the transcript with per-message usage.
// We drive OpenCode; we do not reimplement it.

// HardenedEnv returns the air-gap launch environment with the rtmx intent layer
// wired (exported for the pass-through namespaces, SURFACE-003).
func HardenedEnv(cfg config.Config) []string { return airgapEnv(cfg, true) }

// airgapEnv is the hardened launch environment shared by the TUI launch and the
// serve API: air-gap markers + the operator's model rendered inline (OC-006).
// intent controls whether rtmx is wired as the MCP intent layer (BENCH-004).
func airgapEnv(cfg config.Config, intent bool) []string {
	env := []string{
		"OPENCODE_AUTOUPDATE=0",
		"OPENCODE_TELEMETRY=0",
		"OPENCODE_DISABLE_SHARE=1",
		"OPENAI_BASE_URL=" + cfg.Endpoint + "/v1",
		"OPENAI_API_KEY=not-needed-loopback",
		"OPENCODE_CONFIG_CONTENT=" + RenderConfig(cfg, intent),
	}
	// Put the bundled ripgrep first on PATH so OpenCode's which("rg") resolves it
	// instead of downloading ripgrep from github at bootstrap (OC-009). This env is
	// appended after os.Environ(), so the later PATH wins.
	if p := hardenedPath(); p != "" {
		env = append(env, "PATH="+p)
	}
	// Point OpenCode's config dir at a pre-seeded directory so its bootstrap finds
	// @opencode-ai/plugin already installed and makes no npm-registry request
	// (OC-010). OPENCODE_CONFIG_DIR also becomes Global.Path.config, so the seed is
	// the only config-scope install target.
	if seed, ok := ConfigSeedDir(); ok {
		env = append(env, "OPENCODE_CONFIG_DIR="+seed)
	}
	return env
}

// Tokens is the per-message usage OpenCode reports (used for intent-bench).
type Tokens struct {
	Total     float64 `json:"total"`
	Input     float64 `json:"input"`
	Output    float64 `json:"output"`
	Reasoning float64 `json:"reasoning"`
}

// TranscriptMessage is one flattened message from a session (role + usage + text).
type TranscriptMessage struct {
	Role   string
	Tokens Tokens
	Cost   float64
	Finish string
	Text   string
}

// ServeClient is an HTTP client for a running `opencode serve` API.
type ServeClient struct {
	base  string
	hc    *http.Client
	auth  string // "Basic ..." header value, when the server requires it
	dir   string // working directory the session is rooted at
	agent string // OpenCode agent (e.g. "build")
}

// NewServeClient returns a client for the serve API at base (e.g.
// "http://127.0.0.1:8099"). Exported so tests can drive it against a mock.
func NewServeClient(base string) *ServeClient {
	return &ServeClient{base: base, hc: &http.Client{Timeout: 30 * time.Minute}, agent: "build"}
}

// SetAuth sets HTTP Basic credentials for the session routes.
func (c *ServeClient) SetAuth(username, password string) {
	if password == "" {
		return
	}
	c.auth = "Basic " + base64.StdEncoding.EncodeToString([]byte(username+":"+password))
}

func (c *ServeClient) do(ctx context.Context, method, path string, body, out any) error {
	var rdr io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return err
		}
		rdr = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, c.base+path, rdr)
	if err != nil {
		return err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if c.auth != "" {
		req.Header.Set("Authorization", c.auth)
	}
	resp, err := c.hc.Do(req)
	if err != nil {
		return fmt.Errorf("opencode serve %s %s: %w", method, path, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		b, _ := io.ReadAll(io.LimitReader(resp.Body, 2048))
		return fmt.Errorf("opencode serve %s %s: %s: %s", method, path, resp.Status, b)
	}
	if out != nil {
		return json.NewDecoder(resp.Body).Decode(out)
	}
	return nil
}

// Ready reports whether the serve API is up. GET /openapi.json answers 200 with the
// web UI (HTML) once the server is listening, so we only need the 2xx — no JSON
// body to decode.
func (c *ServeClient) Ready(ctx context.Context) bool {
	return c.do(ctx, http.MethodGet, "/openapi.json", nil, nil) == nil
}

// CreateSession opens a session rooted at the working dir with the agent + model.
func (c *ServeClient) CreateSession(ctx context.Context, model Model) (string, error) {
	body := map[string]any{
		"agent":    c.agent,
		"model":    map[string]any{"providerID": model.ProviderID, "id": model.ModelID},
		"location": map[string]any{"directory": c.dir},
	}
	// POST /session returns the session object flat (NOT wrapped in {"data":...},
	// which is the v2 /api surface).
	var resp struct {
		ID string `json:"id"`
	}
	if err := c.do(ctx, http.MethodPost, "/session", body, &resp); err != nil {
		return "", err
	}
	if resp.ID == "" {
		return "", errors.New("opencode serve: empty session id")
	}
	return resp.ID, nil
}

// Model identifies the provider+model for a prompt.
type Model struct {
	ProviderID string
	ModelID    string
}

// SendMessage posts a prompt to the session's SYNCHRONOUS executor (POST
// /session/{id}/message) and blocks until the autonomous turn completes. This is
// the real headless-run route — not /api/session/{id}/prompt (which only queues)
// and not /wait (an upstream stub). The synchronous response is the assistant
// message; callers read the full transcript with usage via Messages.
func (c *ServeClient) SendMessage(ctx context.Context, sessionID string, model Model, text string) error {
	body := map[string]any{
		"parts": []map[string]any{{"type": "text", "text": text}},
		"model": map[string]any{"providerID": model.ProviderID, "modelID": model.ModelID},
		"agent": c.agent,
	}
	return c.do(ctx, http.MethodPost, "/session/"+sessionID+"/message", body, nil)
}

// Drive runs one autonomous turn synchronously: create a session, post the prompt
// to the synchronous executor (blocks until the turn completes), then collect the
// transcript with per-message usage. Loopback-only (BENCH-006).
func (c *ServeClient) Drive(ctx context.Context, model Model, prompt string) (*SolveResult, error) {
	id, err := c.CreateSession(ctx, model)
	if err != nil {
		return nil, err
	}
	if err := c.SendMessage(ctx, id, model, prompt); err != nil {
		return nil, err
	}
	msgs, err := c.Messages(ctx, id)
	if err != nil {
		return nil, err
	}
	return &SolveResult{SessionID: id, Messages: msgs}, nil
}

// Messages returns the session transcript, flattened to role + usage + text.
func (c *ServeClient) Messages(ctx context.Context, sessionID string) ([]TranscriptMessage, error) {
	// GET /session/{id}/message returns a flat top-level array of messages (NOT
	// wrapped in {"data":...}).
	var data []struct {
		Info struct {
			Role   string  `json:"role"`
			Tokens Tokens  `json:"tokens"`
			Cost   float64 `json:"cost"`
			Finish string  `json:"finish"`
		} `json:"info"`
		Parts []struct {
			Type string `json:"type"`
			Text string `json:"text"`
		} `json:"parts"`
	}
	if err := c.do(ctx, http.MethodGet, "/session/"+sessionID+"/message", nil, &data); err != nil {
		return nil, err
	}
	out := make([]TranscriptMessage, 0, len(data))
	for _, m := range data {
		tm := TranscriptMessage{Role: m.Info.Role, Tokens: m.Info.Tokens, Cost: m.Info.Cost, Finish: m.Info.Finish}
		for _, p := range m.Parts {
			if p.Type == "text" {
				tm.Text += p.Text
			}
		}
		out = append(out, tm)
	}
	return out, nil
}

// statePassword reads the serve password OpenCode generated into its state dir.
func statePassword() string {
	dir := os.Getenv("XDG_STATE_HOME")
	if dir == "" {
		if home, err := os.UserHomeDir(); err == nil {
			dir = filepath.Join(home, ".local", "state")
		}
	}
	b, err := os.ReadFile(filepath.Join(dir, "opencode", "password"))
	if err != nil {
		return ""
	}
	return string(bytes.TrimSpace(b))
}

// StartServe launches `opencode serve` rooted at workdir under the hardened,
// operator-rendered config, waits for readiness, and authenticates with the
// generated state password. The returned stop function terminates the server.
func StartServe(ctx context.Context, bin string, cfg config.Config, workdir string, port int) (*ServeClient, func(), error) {
	cmd := exec.CommandContext(ctx, bin, "serve", "--hostname", "127.0.0.1", "--port", strconv.Itoa(port))
	cmd.Dir = workdir
	cmd.Env = append(os.Environ(), airgapEnv(cfg, true)...)
	cmd.Stdout, cmd.Stderr = os.Stderr, os.Stderr
	if err := cmd.Start(); err != nil {
		return nil, nil, fmt.Errorf("opencode serve: start: %w", err)
	}
	stop := func() {
		_ = cmd.Process.Kill()
		_, _ = cmd.Process.Wait()
	}
	c := NewServeClient(fmt.Sprintf("http://127.0.0.1:%d", port))
	c.dir = workdir
	deadline := time.Now().Add(30 * time.Second)
	for time.Now().Before(deadline) {
		if c.Ready(ctx) {
			c.SetAuth("opencode", statePassword())
			return c, stop, nil
		}
		time.Sleep(200 * time.Millisecond)
	}
	stop()
	return nil, nil, errors.New("opencode serve: API did not become ready within 30s")
}
