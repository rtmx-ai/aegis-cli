package opencode

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// fakeOpenCode writes an executable stub and returns its path.
func fakeOpenCode(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	p := filepath.Join(dir, "opencode")
	if err := os.WriteFile(p, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return p
}

// repoFile reads a path relative to the module root.
func repoFile(t *testing.T, rel string) string {
	t.Helper()
	_, file, _, _ := runtime.Caller(0)
	root := filepath.Dir(filepath.Dir(filepath.Dir(file))) // internal/opencode -> root
	b, err := os.ReadFile(filepath.Join(root, rel))
	if err != nil {
		t.Fatalf("read %s: %v", rel, err)
	}
	return string(b)
}

func envHas(cmdEnv []string, kv string) bool {
	for _, e := range cmdEnv {
		if e == kv {
			return true
		}
	}
	return false
}

// TestResolveAndCommand → REQ-TUI-001: resolve the binary + build the launch.
func TestResolveAndCommand(t *testing.T) {
	bin := fakeOpenCode(t)
	got, err := ResolveBinary(bin)
	if err != nil || got != bin {
		t.Fatalf("ResolveBinary(explicit): got %q err %v", got, err)
	}
	cmd := Command(config.Default(), bin, "")
	if cmd.Path != bin || len(cmd.Args) == 0 || cmd.Args[0] != bin {
		t.Errorf("Command must exec the opencode binary, got path=%q args=%v", cmd.Path, cmd.Args)
	}
}

// TestLaunchUsesLoopbackModel → REQ-TUI-002: the launch points at the loopback model.
func TestLaunchUsesLoopbackModel(t *testing.T) {
	cfg := config.Default() // loopback endpoint
	cmd := Command(cfg, fakeOpenCode(t), "")
	if !envHas(cmd.Env, "OPENAI_BASE_URL="+cfg.Endpoint+"/v1") {
		t.Errorf("launch must set the loopback model base URL; env=%v", cmd.Env)
	}
	if strings.Contains(strings.Join(cmd.Env, " "), "api.openai.com") {
		t.Error("launch must not point at a remote provider")
	}
}

// TestLaunchWiresRtmxMCP → REQ-TUI-003: rtmx is wired as the MCP intent layer.
func TestLaunchWiresRtmxMCP(t *testing.T) {
	cmd := Command(config.Default(), fakeOpenCode(t), "")
	if !envHas(cmd.Env, "OPENCODE_CONFIG="+DefaultConfigPath) {
		t.Errorf("launch must use the hardened config %q", DefaultConfigPath)
	}
	cfg := repoFile(t, DefaultConfigPath)
	if !strings.Contains(cfg, "rtmx") || !strings.Contains(cfg, "mcp-server") {
		t.Error("hardened opencode config must register the rtmx MCP server")
	}
}

// TestLaunchIsHardened → REQ-TUI-004: offline / telemetry-off launch + config.
func TestLaunchIsHardened(t *testing.T) {
	cmd := Command(config.Default(), fakeOpenCode(t), "")
	for _, marker := range []string{"OPENCODE_AUTOUPDATE=0", "OPENCODE_TELEMETRY=0", "OPENCODE_DISABLE_SHARE=1"} {
		if !envHas(cmd.Env, marker) {
			t.Errorf("launch env must set %q", marker)
		}
	}
	cfg := repoFile(t, DefaultConfigPath)
	for _, want := range []string{`"offline": true`, `"telemetry": false`, `"autoupdate": false`} {
		if !strings.Contains(cfg, want) {
			t.Errorf("hardened config must set %s", want)
		}
	}
}

// TestMissingBinaryGuidance → REQ-TUI-006: absent binary fails with guidance.
func TestMissingBinaryGuidance(t *testing.T) {
	// An explicit non-existent path is a missing-binary error.
	if _, err := ResolveBinary(filepath.Join(t.TempDir(), "nope")); !IsMissing(err) {
		t.Error("a non-executable explicit path must be ErrMissing")
	}
	if !strings.Contains(MissingGuidance, "stage") && !strings.Contains(MissingGuidance, "Stage") {
		t.Error("guidance must explain how to stage/bundle OpenCode")
	}
}

// TestResolveStaged → REQ-OC-004: aegis resolves the self-built OpenCode binary
// at the staged path (deploy/opencode/bin/opencode).
func TestResolveStaged(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)
	staged := filepath.Join(dir, StagedRelPath)
	if err := os.MkdirAll(filepath.Dir(staged), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(staged, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	got, err := ResolveBinary("")
	if err != nil {
		t.Fatalf("ResolveBinary should find the staged binary: %v", err)
	}
	if filepath.Base(got) != "opencode" {
		t.Errorf("resolved %q, want the staged opencode", got)
	}
}

// TestOpenCodeConfigConforms → REQ-OC-004 (config): the hardened config uses the
// opencode schema with a loopback provider, rtmx MCP, and offline hardening.
func TestOpenCodeConfigConforms(t *testing.T) {
	cfg := repoFile(t, DefaultConfigPath)
	for _, want := range []string{"opencode.ai/config.json", "127.0.0.1", "rtmx", `"offline": true`} {
		if !strings.Contains(cfg, want) {
			t.Errorf("hardened opencode config must contain %q", want)
		}
	}
}
