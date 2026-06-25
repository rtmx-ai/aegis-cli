package opencode

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

func TestHardenedEnv(t *testing.T) {
	cfg := config.Default()
	cfg.Endpoint = "http://127.0.0.1:11434"
	cfg.ModelID = "phi4-mini:latest"
	joined := strings.Join(HardenedEnv(cfg), "\n")
	for _, w := range []string{"OPENCODE_TELEMETRY=0", "OPENAI_BASE_URL=http://127.0.0.1:11434/v1", "OPENCODE_CONFIG_CONTENT="} {
		if !strings.Contains(joined, w) {
			t.Errorf("HardenedEnv missing %q", w)
		}
	}
}

func TestSetAuth(t *testing.T) {
	c := NewServeClient("http://x")
	c.SetAuth("opencode", "secret")
	if !strings.HasPrefix(c.auth, "Basic ") {
		t.Errorf("auth not set: %q", c.auth)
	}
	c2 := NewServeClient("http://x")
	c2.SetAuth("opencode", "")
	if c2.auth != "" {
		t.Error("empty password must not set auth")
	}
}

func TestStatePassword(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	if err := os.MkdirAll(filepath.Join(dir, "opencode"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "opencode", "password"), []byte("pw123\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if got := statePassword(); got != "pw123" {
		t.Errorf("statePassword = %q, want pw123", got)
	}
}

// TestSolveResolvesAndRuns covers Solve end-to-end against a fake opencode at the
// staged path (resolve -> RunHeadless -> transcript).
func TestSolveResolvesAndRuns(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)
	staged := filepath.Join(dir, StagedRelPath)
	if err := os.MkdirAll(filepath.Dir(staged), 0o755); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"step_finish\",\"part\":{\"reason\":\"stop\",\"text\":\"ok\"}}'\n"
	if err := os.WriteFile(staged, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	res, err := Solve(context.Background(), config.Default(), "", SolveOptions{Workdir: dir, Prompt: "x", Model: "m"})
	if err != nil {
		t.Fatal(err)
	}
	if len(res.Messages) == 0 || res.Messages[0].Finish != "stop" {
		t.Errorf("Solve transcript wrong: %+v", res.Messages)
	}
}
