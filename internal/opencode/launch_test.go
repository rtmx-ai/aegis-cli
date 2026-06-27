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

// TestLaunchWiresRtmxMCP → REQ-TUI-003: rtmx is wired as the MCP intent layer,
// via the inline rendered config OpenCode 2.0 honors (OPENCODE_CONFIG_CONTENT).
func TestLaunchWiresRtmxMCP(t *testing.T) {
	cmd := Command(config.Default(), fakeOpenCode(t), "")
	var content string
	for _, e := range cmd.Env {
		if strings.HasPrefix(e, "OPENCODE_CONFIG_CONTENT=") {
			content = e
		}
	}
	if content == "" {
		t.Fatal("launch must pass the rendered config via OPENCODE_CONFIG_CONTENT")
	}
	if !strings.Contains(content, "rtmx") || !strings.Contains(content, "mcp-server") {
		t.Error("rendered opencode config must register the rtmx MCP server")
	}
}

// TestRenderConfig → REQ-OC-006: the rendered config targets the operator's
// loopback endpoint + model, with rtmx MCP and offline hardening.
func TestRenderConfig(t *testing.T) {
	cfg := config.Default()
	cfg.Endpoint = "http://127.0.0.1:11434"
	cfg.ModelID = "phi4-mini:latest"
	got := RenderConfig(cfg, true)
	for _, want := range []string{
		`"baseURL": "http://127.0.0.1:11434/v1"`, // operator endpoint
		"phi4-mini:latest",                       // operator model
		`"model": "local/phi4-mini:latest"`,
		"mcp-server", "rtmx", // intent layer
		`"autoupdate": false`, `"share": "disabled"`, // hardening (classic schema)
	} {
		if !strings.Contains(got, want) {
			t.Errorf("rendered config must contain %q\n--- got ---\n%s", want, got)
		}
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
	for _, want := range []string{`"autoupdate": false`, `"share": "disabled"`} {
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
	for _, want := range []string{"opencode.ai/config.json", "127.0.0.1", "rtmx", `"autoupdate": false`} {
		if !strings.Contains(cfg, want) {
			t.Errorf("hardened opencode config must contain %q", want)
		}
	}
}

// TestRunAgentCoaching → REQ-RUNQ-002: the rendered config wires a tool-call
// coaching instruction into the agent's system prompt, and that instruction is
// staged with explicit tool-use directives — so small local models call tools
// instead of emitting prose (the failure observed in the bake-off).
func TestRunAgentCoaching(t *testing.T) {
	t.Chdir(t.TempDir())
	got := RenderConfig(config.Default(), true)

	// The rendered config must wire the coaching instruction file.
	if !strings.Contains(got, `"instructions"`) || !strings.Contains(got, toolCoachingFile) {
		t.Fatalf("rendered config must wire the tool-coaching instruction:\n%s", got)
	}

	// The instruction file must be staged with concrete tool-use directives.
	seed, ok := ConfigSeedDir()
	if !ok {
		t.Fatal("ConfigSeedDir did not stage")
	}
	t.Cleanup(func() { _ = os.RemoveAll(seed) })
	b, err := os.ReadFile(filepath.Join(seed, toolCoachingFile))
	if err != nil {
		t.Fatalf("coaching file not staged: %v", err)
	}
	body := strings.ToLower(string(b))
	for _, want := range []string{"edit", "write", "bash", "tool", "prose"} {
		if !strings.Contains(body, want) {
			t.Errorf("coaching file missing directive %q", want)
		}
	}
}

// TestPerModelTuning → REQ-SERVE-020: when a config carries per-model tuning, the
// rendered OpenCode config applies it to the build agent (temperature/top_p) + the
// Ollama-extension options (top_k/min_p/repeat_penalty/num_ctx/think) — so the model
// emits reliable tool calls. No tuning renders no agent block (default behavior).
func TestPerModelTuning(t *testing.T) {
	temp, topp, minp, rep := 0.7, 0.8, 0.0, 1.05
	topk, ctx := 20, 16384
	think := false
	cfg := config.Default()
	cfg.ModelID = "qwen3-coder:30b"
	cfg.Tuning = &config.ModelTuning{
		Temperature: &temp, TopP: &topp, TopK: &topk, MinP: &minp,
		RepeatPenalty: &rep, NumCtx: &ctx, Think: &think,
	}
	got := RenderConfig(cfg, true)
	for _, want := range []string{
		`"agent"`, `"build"`, `"temperature":0.7`, `"top_p":0.8`,
		`"options"`, `"top_k":20`, `"num_ctx":16384`, `"repeat_penalty":1.05`, `"think":false`,
	} {
		if !strings.Contains(got, want) {
			t.Errorf("rendered config must carry per-model tuning %q\n--- got ---\n%s", want, got)
		}
	}
	cfg.Tuning = nil
	if strings.Contains(RenderConfig(cfg, true), `"agent"`) {
		t.Error("no tuning must render no agent block (default behavior unchanged)")
	}
}
