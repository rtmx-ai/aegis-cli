// Package serving is the built-in serving-backed harness: it drives a single
// requirement headless by asking the local model (over the loopback serving
// client) for file edits, applying them atomically inside the workspace, and
// running the requirement's acceptance test. It is the production code-gen path
// that needs no external harness binary.
//
// See docs/requirements/harness-serving.md (HARNESS-003..010). The shipped
// binary stays std-lib-only; the only network path is the loopback serving
// client, which refuses non-loopback hosts at construction.
package serving

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// systemPrompt instructs the model to emit edits in a strict, parseable form.
const systemPrompt = `You are a disciplined implementer. Make the minimal change to satisfy the requirement and its acceptance test. Reply ONLY with one or more file blocks in this exact form:
` + "```" + `file <relative/path>
<full file contents>
` + "```" + `
Do not include prose outside the file blocks.`

// TestRunner runs a requirement's acceptance test in workspace and reports
// pass/fail. It is injectable so the harness can be tested without invoking a
// real toolchain; the default runs the module test suite.
type TestRunner func(ctx context.Context, workspace string, req *rtmx.Requirement) (bool, error)

// Adapter is the built-in serving-backed harness.Adapter.
type Adapter struct {
	client          *serving.Client
	model           string
	workspace       string
	runTest         TestRunner
	maxParseRetries int
}

// Option configures an Adapter.
type Option func(*Adapter)

// WithModel sets the model name sent to the endpoint.
func WithModel(m string) Option { return func(a *Adapter) { a.model = m } }

// WithWorkspace sets the directory the harness may modify.
func WithWorkspace(dir string) Option { return func(a *Adapter) { a.workspace = dir } }

// WithTestRunner overrides the acceptance-test runner (tests inject a fake).
func WithTestRunner(r TestRunner) Option { return func(a *Adapter) { a.runTest = r } }

// WithMaxParseRetries bounds the in-Drive retries on malformed model output.
func WithMaxParseRetries(n int) Option { return func(a *Adapter) { a.maxParseRetries = n } }

func newAdapter(opts ...Option) *Adapter {
	a := &Adapter{model: "local", workspace: ".", maxParseRetries: 2, runTest: defaultTestRunner}
	for _, o := range opts {
		o(a)
	}
	return a
}

// New builds an Adapter against a loopback endpoint. A non-loopback endpoint is
// refused at construction (HARNESS-009).
func New(endpoint string, opts ...Option) (*Adapter, error) {
	c, err := serving.NewClient(endpoint)
	if err != nil {
		return nil, err
	}
	a := newAdapter(opts...)
	a.client = c
	return a, nil
}

// NewWithClient injects an already-constructed client (used in tests).
func NewWithClient(c *serving.Client, opts ...Option) *Adapter {
	a := newAdapter(opts...)
	a.client = c
	return a
}

// Name reports the adapter identity.
func (a *Adapter) Name() string { return "builtin" }

// Health probes the endpoint by reading model info over loopback.
func (a *Adapter) Health(ctx context.Context) error {
	_, err := a.client.ModelInfo(ctx)
	return err
}

// Drive implements harness.Adapter: prompt → model → parse (retry on malformed)
// → workspace-sandboxed atomic apply → acceptance test, rolling back on any
// failure. A returned error makes the loop treat the attempt as failed.
func (a *Adapter) Drive(ctx context.Context, req *rtmx.Requirement) (harness.Diff, error) {
	diff := harness.Diff{RequirementID: req.ID}
	prompt := buildPrompt(req)

	var edits []fileEdit
	var parseErr error
	for attempt := 0; attempt <= a.maxParseRetries; attempt++ {
		resp, err := a.client.ChatCompletion(ctx, serving.ChatRequest{
			Model: a.model,
			Messages: []serving.Message{
				{Role: "system", Content: systemPrompt},
				{Role: "user", Content: prompt},
			},
		})
		if err != nil {
			return diff, fmt.Errorf("serving: completion: %w", err)
		}
		content := firstContent(resp)
		diff.Turns++
		diff.ToolCalls++
		diff.Tokens += len(content)
		edits, parseErr = parseEdits(content)
		if parseErr == nil {
			diff.ValidToolCalls++
			break
		}
		// Malformed output: detected, not crashed — retry within budget.
	}
	if parseErr != nil {
		return diff, fmt.Errorf("serving: unparseable model output after %d attempts: %w", a.maxParseRetries+1, parseErr)
	}

	// Workspace sandbox: refuse any out-of-workspace target before writing.
	for _, e := range edits {
		if err := safePath(a.workspace, e.Path); err != nil {
			return diff, err
		}
	}

	// Atomic apply with rollback on a failed apply or failed acceptance test.
	snap, err := applyEdits(a.workspace, edits)
	if err != nil {
		snap.rollback()
		return diff, fmt.Errorf("serving: apply: %w", err)
	}
	ok, terr := a.runTest(ctx, a.workspace, req)
	if terr != nil {
		snap.rollback()
		return diff, fmt.Errorf("serving: acceptance test: %w", terr)
	}
	if !ok {
		snap.rollback()
		return diff, errors.New("serving: acceptance test failed")
	}

	diff.Patch = renderPatch(edits)
	return diff, nil
}

// buildPrompt produces a lean, scoped prompt (HARNESS-004): the requirement and
// its acceptance-test references only — no unrelated repo context.
func buildPrompt(req *rtmx.Requirement) string {
	var b strings.Builder
	fmt.Fprintf(&b, "Requirement %s: %s\n", req.ID, req.Title)
	if len(req.Tests) > 0 {
		fmt.Fprintf(&b, "Acceptance tests: %s\n", strings.Join(req.Tests, ", "))
	}
	b.WriteString("Implement the minimal change so the acceptance test passes.")
	return b.String()
}

// fileEdit is one file to write in full.
type fileEdit struct {
	Path    string
	Content string
}

// parseEdits extracts file blocks of the form ```file <path> ... ``` from model
// output (HARNESS-005). Output with no valid block is a parse error (the caller
// retries). Unterminated blocks are rejected.
func parseEdits(content string) ([]fileEdit, error) {
	lines := strings.Split(content, "\n")
	var edits []fileEdit
	for i := 0; i < len(lines); i++ {
		line := strings.TrimRight(lines[i], "\r")
		if !strings.HasPrefix(strings.TrimSpace(line), "```file") {
			continue
		}
		path := strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(line), "```file"))
		if path == "" {
			return nil, errors.New("file block missing a path")
		}
		// Collect body until the closing fence.
		var body []string
		closed := false
		for j := i + 1; j < len(lines); j++ {
			if strings.TrimSpace(strings.TrimRight(lines[j], "\r")) == "```" {
				i = j
				closed = true
				break
			}
			body = append(body, lines[j])
		}
		if !closed {
			return nil, fmt.Errorf("file block for %q is not terminated", path)
		}
		content := strings.Join(body, "\n")
		if len(body) > 0 {
			content += "\n"
		}
		edits = append(edits, fileEdit{Path: path, Content: content})
	}
	if len(edits) == 0 {
		return nil, errors.New("no file edits found in model output")
	}
	return edits, nil
}

// safePath rejects edit targets outside the workspace (HARNESS-006): absolute
// paths and any path that resolves above the workspace root.
func safePath(workspace, p string) error {
	if filepath.IsAbs(p) {
		return fmt.Errorf("serving: refusing absolute path %q (outside workspace)", p)
	}
	clean := filepath.Clean(p)
	rel, err := filepath.Rel(workspace, filepath.Join(workspace, clean))
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return fmt.Errorf("serving: refusing path %q (escapes workspace)", p)
	}
	return nil
}

// snapshot records pre-apply file state so an attempt can be rolled back.
type snapshot struct {
	prior []priorFile
}

type priorFile struct {
	path    string
	existed bool
	content []byte
}

// applyEdits writes each edit, recording prior state for rollback (HARNESS-007).
func applyEdits(workspace string, edits []fileEdit) (*snapshot, error) {
	s := &snapshot{}
	for _, e := range edits {
		abs := filepath.Join(workspace, filepath.Clean(e.Path))
		pf := priorFile{path: abs}
		if b, err := os.ReadFile(abs); err == nil {
			pf.existed = true
			pf.content = b
		}
		s.prior = append(s.prior, pf)
		if err := os.MkdirAll(filepath.Dir(abs), 0o755); err != nil {
			return s, err
		}
		if err := os.WriteFile(abs, []byte(e.Content), 0o644); err != nil {
			return s, err
		}
	}
	return s, nil
}

// rollback restores the working tree to its pre-apply state.
func (s *snapshot) rollback() {
	for i := len(s.prior) - 1; i >= 0; i-- {
		pf := s.prior[i]
		if pf.existed {
			_ = os.WriteFile(pf.path, pf.content, 0o644)
		} else {
			_ = os.Remove(pf.path)
		}
	}
}

// renderPatch summarizes the applied edits as a short unified-diff-style header.
func renderPatch(edits []fileEdit) string {
	var b strings.Builder
	for _, e := range edits {
		fmt.Fprintf(&b, "--- a/%s\n+++ b/%s\n", e.Path, e.Path)
	}
	return b.String()
}

// firstContent returns the first choice's message content, if any.
func firstContent(resp serving.ChatResponse) string {
	if len(resp.Choices) > 0 {
		return resp.Choices[0].Message.Content
	}
	return ""
}

// defaultTestRunner runs the module test suite in the workspace; exit 0 = pass.
func defaultTestRunner(ctx context.Context, workspace string, _ *rtmx.Requirement) (bool, error) {
	cmd := exec.CommandContext(ctx, "go", "test", "./...")
	cmd.Dir = workspace
	if err := cmd.Run(); err != nil {
		var ee *exec.ExitError
		if errors.As(err, &ee) {
			return false, nil // test failures are a clean "false", not an error
		}
		return false, err
	}
	return true, nil
}

// compile-time assertion that Adapter satisfies harness.Adapter.
var _ harness.Adapter = (*Adapter)(nil)
