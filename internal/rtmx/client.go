package rtmx

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
)

// VerifyFunc runs verification for a requirement and reports pass/fail. It is
// injectable so the clients can be tested without a toolchain; the default runs
// the requirement's mapped Go test.
type VerifyFunc func(ctx context.Context, id string) (bool, error)

// ClientOption configures a real client.
type ClientOption func(*clientBase)

type clientBase struct {
	store  *Store
	agent  string
	verify VerifyFunc
}

// WithAgent sets the agent id used for claim/release coordination.
func WithAgent(id string) ClientOption { return func(c *clientBase) { c.agent = id } }

// WithVerifyFunc overrides verification (tests inject a fake).
func WithVerifyFunc(v VerifyFunc) ClientOption { return func(c *clientBase) { c.verify = v } }

func newBase(dbPath string, opts ...ClientOption) clientBase {
	b := clientBase{store: NewStore(dbPath), agent: "aegis"}
	for _, o := range opts {
		o(&b)
	}
	if b.verify == nil {
		b.verify = defaultVerify(dbPath)
	}
	return b
}

// repoRootOf returns the module root for a database path (.../.rtmx/database.csv → ...).
func repoRootOf(dbPath string) string {
	abs, err := filepath.Abs(dbPath)
	if err != nil {
		abs = dbPath
	}
	return filepath.Dir(filepath.Dir(abs))
}

// defaultVerify runs the requirement's mapped Go test (module::func) from the
// repo root; a clean exit is a pass, a test failure is a clean false.
func defaultVerify(dbPath string) VerifyFunc {
	store := NewStore(dbPath)
	root := repoRootOf(dbPath)
	return func(ctx context.Context, id string) (bool, error) {
		r, err := store.ByID(id)
		if err != nil {
			return false, err
		}
		if len(r.Tests) == 0 {
			return false, nil
		}
		mod, fn, _ := strings.Cut(r.Tests[0], "::")
		pkg := "./..."
		if mod != "" {
			pkg = "./" + strings.Trim(mod, "/") + "/..."
		}
		args := []string{"test"}
		if fn != "" {
			args = append(args, "-run", "^"+fn+"$")
		}
		args = append(args, pkg)
		cmd := exec.CommandContext(ctx, "go", args...)
		cmd.Dir = root
		if err := cmd.Run(); err != nil {
			var ee *exec.ExitError
			if errors.As(err, &ee) {
				return false, nil
			}
			return false, err
		}
		return true, nil
	}
}

// --- CLI client (RTMX-005) --------------------------------------------------

// CLIClient is a real Client backed by the CSV store, with health delegated to
// the rtmx binary. It is the fallback path when the MCP server is unavailable.
type CLIClient struct {
	clientBase
	health func(ctx context.Context) error
}

// NewCLIClient builds a CSV+CLI-backed client for the database at dbPath.
func NewCLIClient(dbPath string, opts ...ClientOption) *CLIClient {
	return &CLIClient{clientBase: newBase(dbPath, opts...), health: rtmxHealth(dbPath)}
}

func (c *CLIClient) Next(ctx context.Context) (*Requirement, error) { return c.store.Next() }
func (c *CLIClient) Claim(ctx context.Context, id string) error     { return c.store.Claim(id, c.agent) }
func (c *CLIClient) Release(ctx context.Context, id string) error   { return c.store.Release(id) }
func (c *CLIClient) Verify(ctx context.Context, id string) (bool, error) {
	return c.verify(ctx, id)
}
func (c *CLIClient) WriteStatus(ctx context.Context, id string, s Status) error {
	return c.store.SetStatus(id, s)
}
func (c *CLIClient) Health(ctx context.Context) error { return c.health(ctx) }

// rtmxHealth shells `rtmx health` from the repo root; a clean exit means healthy.
func rtmxHealth(dbPath string) func(ctx context.Context) error {
	root := repoRootOf(dbPath)
	return func(ctx context.Context) error {
		cmd := exec.CommandContext(ctx, "rtmx", "health")
		cmd.Dir = root
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("rtmx health: %w", err)
		}
		return nil
	}
}

// --- MCP stdio client (RTMX-004) --------------------------------------------

// MCPClient speaks JSON-RPC over stdio to `rtmx mcp-server --stdio` for the
// coordination tools rtmx exposes (next/claim/release/health). Requirement
// detail and status writeback go through the shared CSV store, since the MCP
// surface has no status-writeback tool and `next` returns ids only.
type MCPClient struct {
	clientBase
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout *bufio.Reader
	mu     sync.Mutex
	nextID int
}

// DialMCP launches the rtmx MCP server against dbPath's repo and completes the
// initialize handshake.
func DialMCP(ctx context.Context, dbPath string, opts ...ClientOption) (*MCPClient, error) {
	cmd := exec.Command("rtmx", "mcp-server", "--stdio")
	cmd.Dir = repoRootOf(dbPath)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("rtmx mcp-server: start: %w", err)
	}
	c := &MCPClient{
		clientBase: newBase(dbPath, opts...),
		cmd:        cmd,
		stdin:      stdin,
		stdout:     bufio.NewReader(stdout),
	}
	if _, err := c.call("initialize", map[string]any{
		"protocolVersion": "2024-11-05",
		"capabilities":    map[string]any{},
		"clientInfo":      map[string]any{"name": "aegis", "version": "0"},
	}); err != nil {
		c.Close()
		return nil, fmt.Errorf("rtmx mcp-server: initialize: %w", err)
	}
	return c, nil
}

// Close terminates the MCP server subprocess.
func (c *MCPClient) Close() error {
	_ = c.stdin.Close()
	if c.cmd.Process != nil {
		_ = c.cmd.Process.Kill()
	}
	return c.cmd.Wait()
}

// call sends one JSON-RPC request and returns the matching response result.
func (c *MCPClient) call(method string, params any) (json.RawMessage, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.nextID++
	id := c.nextID
	req, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": id, "method": method, "params": params})
	if _, err := c.stdin.Write(append(req, '\n')); err != nil {
		return nil, err
	}
	for {
		line, err := c.stdout.ReadBytes('\n')
		if err != nil {
			return nil, err
		}
		var resp struct {
			ID     *int            `json:"id"`
			Result json.RawMessage `json:"result"`
			Error  *struct {
				Message string `json:"message"`
			} `json:"error"`
		}
		if json.Unmarshal(line, &resp) != nil || resp.ID == nil || *resp.ID != id {
			continue // skip notifications / unrelated lines
		}
		if resp.Error != nil {
			return nil, fmt.Errorf("rtmx mcp: %s", resp.Error.Message)
		}
		return resp.Result, nil
	}
}

// toolCall invokes an MCP tool and returns its inner text payload (JSON).
func (c *MCPClient) toolCall(name string, args map[string]any) (json.RawMessage, error) {
	raw, err := c.call("tools/call", map[string]any{"name": name, "arguments": args})
	if err != nil {
		return nil, err
	}
	var res struct {
		Content []struct {
			Text string `json:"text"`
		} `json:"content"`
		IsError bool `json:"isError"`
	}
	if err := json.Unmarshal(raw, &res); err != nil {
		return nil, err
	}
	if len(res.Content) == 0 {
		return nil, nil
	}
	if res.IsError {
		return nil, fmt.Errorf("rtmx mcp %s: %s", name, res.Content[0].Text)
	}
	return json.RawMessage(res.Content[0].Text), nil
}

// Next asks the MCP server for the top unblocked work item, then loads detail
// from the CSV store.
func (c *MCPClient) Next(ctx context.Context) (*Requirement, error) {
	raw, err := c.toolCall("next", map[string]any{})
	if err != nil {
		return nil, err
	}
	var webs struct {
		Webs []struct {
			Unblocked int    `json:"unblocked"`
			TopItem   string `json:"top_item"`
		} `json:"webs"`
	}
	if err := json.Unmarshal(raw, &webs); err != nil {
		return nil, err
	}
	for _, w := range webs.Webs {
		if w.Unblocked > 0 && w.TopItem != "" {
			return c.store.ByID(w.TopItem)
		}
	}
	return nil, nil
}

// Claim claims via the MCP server (atomic, multi-agent safe).
func (c *MCPClient) Claim(ctx context.Context, id string) error {
	_, err := c.toolCall("claim", map[string]any{"req_id": id, "agent_id": c.agent})
	return err
}

// Release releases via the MCP server.
func (c *MCPClient) Release(ctx context.Context, id string) error {
	_, err := c.toolCall("release", map[string]any{"req_id": id, "agent_id": c.agent})
	return err
}

// Verify runs the injected/default verifier (the MCP verify tool runs the whole
// suite; per-requirement verification stays in the shared verifier for parity).
func (c *MCPClient) Verify(ctx context.Context, id string) (bool, error) { return c.verify(ctx, id) }

// WriteStatus writes through the CSV store (no MCP status-writeback tool exists).
func (c *MCPClient) WriteStatus(ctx context.Context, id string, s Status) error {
	return c.store.SetStatus(id, s)
}

// Health calls the MCP health tool.
func (c *MCPClient) Health(ctx context.Context) error {
	raw, err := c.toolCall("health", map[string]any{})
	if err != nil {
		return err
	}
	var h struct {
		Status string `json:"status"`
	}
	if err := json.Unmarshal(raw, &h); err != nil {
		return err
	}
	if strings.EqualFold(h.Status, "HEALTHY") || strings.EqualFold(h.Status, "WARNING") {
		return nil
	}
	return fmt.Errorf("rtmx mcp health: %s", h.Status)
}

// compile-time assertions that both clients satisfy Client.
var (
	_ Client = (*CLIClient)(nil)
	_ Client = (*MCPClient)(nil)
)
