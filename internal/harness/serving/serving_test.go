package serving

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// block renders a parseable ```file <path> ... ``` edit block.
func block(path, body string) string {
	return "```file " + path + "\n" + body + "\n```\n"
}

func passRunner(context.Context, string, *rtmx.Requirement) (bool, error) { return true, nil }

// newAdapterAgainst wires an Adapter to a mock model server.
func newAdapterAgainst(t *testing.T, mock *mockmodel.Server, opts ...Option) *Adapter {
	t.Helper()
	c, err := serving.NewClient(mock.URL())
	if err != nil {
		t.Fatalf("client: %v", err)
	}
	return NewWithClient(c, opts...)
}

// TestServingAdapterDrives → HARNESS-003.
func TestServingAdapterDrives(t *testing.T) {
	ws := t.TempDir()
	mock := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{{Content: block("impl.go", "package impl")}}})
	defer mock.Close()
	a := newAdapterAgainst(t, mock, WithWorkspace(ws), WithTestRunner(passRunner))

	diff, err := a.Drive(context.Background(), &rtmx.Requirement{ID: "FEAT-DEMO-1", Title: "do x"}, "")
	if err != nil {
		t.Fatalf("Drive: %v", err)
	}
	if diff.RequirementID != "FEAT-DEMO-1" || diff.Patch == "" {
		t.Errorf("expected a populated diff, got %+v", diff)
	}
	if diff.ValidToolCalls != 1 || diff.Turns != 1 {
		t.Errorf("expected 1 valid turn, got turns=%d valid=%d", diff.Turns, diff.ValidToolCalls)
	}
	if b, err := os.ReadFile(filepath.Join(ws, "impl.go")); err != nil || !strings.Contains(string(b), "package impl") {
		t.Errorf("edit not applied to workspace: %v", err)
	}
}

// TestBuildPromptIsScoped → HARNESS-004.
func TestBuildPromptIsScoped(t *testing.T) {
	p := buildPrompt(&rtmx.Requirement{ID: "X-1", Title: "add foo", Tests: []string{"pkg/foo_test"}})
	for _, want := range []string{"X-1", "add foo", "pkg/foo_test"} {
		if !strings.Contains(p, want) {
			t.Errorf("prompt missing %q:\n%s", want, p)
		}
	}
	if strings.Contains(p, "unrelated") {
		t.Error("prompt should not contain unrelated context")
	}
}

// TestParseEditsRetriesOnMalformed → HARNESS-005.
func TestParseEditsRetriesOnMalformed(t *testing.T) {
	// Direct parse contract.
	if _, err := parseEdits("just some prose, no blocks"); err == nil {
		t.Error("expected malformed output to be a parse error")
	}
	if edits, err := parseEdits(block("a.go", "package a")); err != nil || len(edits) != 1 {
		t.Errorf("expected one parsed edit, got %v err=%v", edits, err)
	}

	// In-Drive retry: malformed first, valid second.
	ws := t.TempDir()
	mock := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{
		{Content: "no blocks here"},
		{Content: block("a.go", "package a")},
	}})
	defer mock.Close()
	a := newAdapterAgainst(t, mock, WithWorkspace(ws), WithTestRunner(passRunner))

	diff, err := a.Drive(context.Background(), &rtmx.Requirement{ID: "R-1", Title: "t"}, "")
	if err != nil {
		t.Fatalf("Drive after retry: %v", err)
	}
	if diff.Turns != 2 || diff.ToolCalls != 2 || diff.ValidToolCalls != 1 {
		t.Errorf("expected one retry then success, got %+v", diff)
	}
}

// TestApplyRejectsOutsideWorkspace → HARNESS-006.
func TestApplyRejectsOutsideWorkspace(t *testing.T) {
	ws := t.TempDir()
	if err := safePath(ws, "/etc/passwd"); err == nil {
		t.Error("absolute path must be rejected")
	}
	if err := safePath(ws, "../escape.txt"); err == nil {
		t.Error("traversal path must be rejected")
	}
	if err := safePath(ws, "sub/ok.go"); err != nil {
		t.Errorf("in-workspace path must be allowed: %v", err)
	}

	mock := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{{Content: block("../escape.txt", "pwned")}}})
	defer mock.Close()
	a := newAdapterAgainst(t, mock, WithWorkspace(ws), WithTestRunner(passRunner))

	if _, err := a.Drive(context.Background(), &rtmx.Requirement{ID: "R-1"}, ""); err == nil {
		t.Error("expected an out-of-workspace edit to be refused")
	}
	if _, err := os.Stat(filepath.Join(filepath.Dir(ws), "escape.txt")); !os.IsNotExist(err) {
		t.Error("out-of-workspace file must not have been written")
	}
}

// TestApplyRollsBackOnFailure → HARNESS-007.
func TestApplyRollsBackOnFailure(t *testing.T) {
	ws := t.TempDir()
	existing := filepath.Join(ws, "existing.txt")
	if err := os.WriteFile(existing, []byte("ORIG"), 0o644); err != nil {
		t.Fatal(err)
	}
	content := block("existing.txt", "MODIFIED") + block("new.txt", "CREATED")
	mock := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{{Content: content}}})
	defer mock.Close()
	// Acceptance test fails → the apply must roll back.
	failRunner := func(context.Context, string, *rtmx.Requirement) (bool, error) { return false, nil }
	a := newAdapterAgainst(t, mock, WithWorkspace(ws), WithTestRunner(failRunner))

	if _, err := a.Drive(context.Background(), &rtmx.Requirement{ID: "R-1"}, ""); err == nil {
		t.Error("expected Drive to fail when the acceptance test fails")
	}
	if b, _ := os.ReadFile(existing); string(b) != "ORIG" {
		t.Errorf("existing file must be restored to ORIG, got %q", b)
	}
	if _, err := os.Stat(filepath.Join(ws, "new.txt")); !os.IsNotExist(err) {
		t.Error("newly-created file must be removed on rollback")
	}
}

// TestRunsTestsAndReportsMetrics → HARNESS-008.
func TestRunsTestsAndReportsMetrics(t *testing.T) {
	ws := t.TempDir()
	mock := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{{Content: block("impl.go", "package impl")}}})
	defer mock.Close()

	var gotWS, gotReq string
	runner := func(_ context.Context, workspace string, req *rtmx.Requirement) (bool, error) {
		gotWS, gotReq = workspace, req.ID
		return true, nil
	}
	a := newAdapterAgainst(t, mock, WithWorkspace(ws), WithTestRunner(runner))

	diff, err := a.Drive(context.Background(), &rtmx.Requirement{ID: "R-9", Title: "t"}, "")
	if err != nil {
		t.Fatalf("Drive: %v", err)
	}
	if gotWS != ws || gotReq != "R-9" {
		t.Errorf("acceptance runner not invoked with workspace+req: ws=%q req=%q", gotWS, gotReq)
	}
	if diff.Turns < 1 || diff.ValidToolCalls < 1 || diff.Tokens <= 0 {
		t.Errorf("expected populated metrics, got %+v", diff)
	}
}

// TestServingHarnessLoopbackOnly → HARNESS-009.
func TestServingHarnessLoopbackOnly(t *testing.T) {
	if _, err := New("http://8.8.8.8:8080"); err == nil {
		t.Error("non-loopback endpoint must be refused")
	}
	if _, err := New("http://127.0.0.1:9"); err != nil {
		t.Errorf("loopback endpoint must be accepted at construction: %v", err)
	}
}
