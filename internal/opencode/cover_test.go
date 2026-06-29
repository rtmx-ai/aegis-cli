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
	for _, w := range []string{"OPENCODE_TELEMETRY=0", "OPENAI_BASE_URL=http://127.0.0.1:11434/v1", "OPENCODE_CONFIG_CONTENT=", "OPENCODE_DISABLE_AUTOUPDATE=1", "OPENCODE_PURE=1"} {
		if !strings.Contains(joined, w) {
			t.Errorf("HardenedEnv missing %q", w)
		}
	}
}

// TestVerifyLaunchMissingBinary covers VerifyLaunch's resolve-error path: with no
// OpenCode staged it returns an error rather than launching anything.
func TestVerifyLaunchMissingBinary(t *testing.T) {
	t.Chdir(t.TempDir())
	if err := VerifyLaunch(context.Background(), config.Default()); err == nil {
		t.Error("VerifyLaunch should error when OpenCode is not resolvable")
	}
}

// TestModelsFetchDisabled → REQ-OC-011: the hardened launch env disables OpenCode's
// models.dev catalog fetch (a non-loopback egress to models.dev at startup).
func TestModelsFetchDisabled(t *testing.T) {
	joined := strings.Join(HardenedEnv(config.Default()), "\n")
	if !strings.Contains(joined, "OPENCODE_DISABLE_MODELS_FETCH=1") {
		t.Error("hardened env must disable the models.dev fetch (OPENCODE_DISABLE_MODELS_FETCH=1)")
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

// TestSolveUsesServeDrive → REQ-BENCH-008: Solve resolves the binary and routes the
// run through the serve drive (not the classic `opencode run` path), passing the
// run options through. The real serve drive is covered end-to-end by the gated
// TestServeDriveRealBinary; here we assert the routing via the seam.
func TestSolveUsesServeDrive(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)
	staged := filepath.Join(dir, StagedRelPath)
	if err := os.MkdirAll(filepath.Dir(staged), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(staged, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	var gotBin string
	var gotOpts SolveOptions
	orig := solveDrive
	t.Cleanup(func() { solveDrive = orig })
	solveDrive = func(_ context.Context, bin string, _ config.Config, opts SolveOptions) (*SolveResult, error) {
		gotBin, gotOpts = bin, opts
		return &SolveResult{SessionID: "ses_x", Messages: []TranscriptMessage{{Role: "assistant", Tokens: Tokens{Total: 5}}}}, nil
	}

	res, err := Solve(context.Background(), config.Default(), "", SolveOptions{Workdir: dir, Prompt: "do x", Model: "m"})
	if err != nil {
		t.Fatal(err)
	}
	if gotBin == "" {
		t.Error("Solve did not resolve the OpenCode binary before driving")
	}
	if gotOpts.Prompt != "do x" || gotOpts.Model != "m" || gotOpts.Workdir != dir {
		t.Errorf("Solve did not pass the run options to the serve drive: %+v", gotOpts)
	}
	if res.SessionID != "ses_x" {
		t.Errorf("Solve did not return the serve drive result: %+v", res)
	}
}
