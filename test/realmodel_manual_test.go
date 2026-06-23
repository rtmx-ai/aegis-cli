package offline

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	servingharness "github.com/rtmx-ai/aegis-cli/internal/harness/serving"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// TestRealModelValidation drives the REAL serving client + built-in harness
// against a live local model. Gated: set AEGIS_REAL_ENDPOINT (loopback) and
// AEGIS_REAL_MODEL. Manual validation only — skipped in CI.
func TestRealModelValidation(t *testing.T) {
	endpoint := os.Getenv("AEGIS_REAL_ENDPOINT")
	model := os.Getenv("AEGIS_REAL_MODEL")
	if endpoint == "" || model == "" {
		t.Skip("set AEGIS_REAL_ENDPOINT + AEGIS_REAL_MODEL for real-model validation")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 120*time.Second)
	defer cancel()

	client, err := serving.NewClient(endpoint, serving.WithTimeout(120*time.Second))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}

	// 1. Preflight smoke against the real model.
	if err := client.PreflightSmoke(ctx, model); err != nil {
		t.Fatalf("preflight smoke failed: %v", err)
	}
	t.Log("preflight smoke: OK")

	// 2. A real chat completion (latency + tokens surfaced).
	resp, err := client.ChatCompletion(ctx, serving.ChatRequest{
		Model:    model,
		Messages: []serving.Message{{Role: "user", Content: "Reply with exactly: PONG"}},
	})
	if err != nil {
		t.Fatalf("chat completion: %v", err)
	}
	t.Logf("completion: %q (latency=%s, tokens=%d)", firstContent(resp), resp.Latency, resp.Usage.TotalTokens)

	// 3. Built-in harness drives a trivial requirement to a real, parseable edit.
	ws := t.TempDir()
	a := servingharness.NewWithClient(client,
		servingharness.WithModel(model),
		servingharness.WithWorkspace(ws),
		servingharness.WithTestRunner(func(context.Context, string, *rtmx.Requirement) (bool, error) { return true, nil }),
	)
	req := &rtmx.Requirement{
		ID:    "REAL-001",
		Title: "Create a Go file greet.go in package demo with a function Greet() string that returns \"hello\".",
		Tests: []string{"demo/greet_test.go::TestGreet"},
	}
	diff, err := a.Drive(ctx, req)
	if err != nil {
		t.Fatalf("harness Drive against real model: %v", err)
	}
	t.Logf("harness produced patch (%d turns, %d tokens):\n%s", diff.Turns, diff.Tokens, diff.Patch)
	// Confirm the model's edit was parsed + applied to the workspace.
	matches, _ := filepath.Glob(filepath.Join(ws, "*"))
	if len(matches) == 0 {
		t.Fatal("no file produced by the real model")
	}
	t.Logf("applied files: %v", matches)
}

func firstContent(r serving.ChatResponse) string {
	if len(r.Choices) > 0 {
		return r.Choices[0].Message.Content
	}
	return ""
}
