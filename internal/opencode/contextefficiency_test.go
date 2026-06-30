package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestContextEfficiencyPluginStaged → PERF-004/005: the bundled context-efficiency plugin carries the
// reasoning-strip (PERF-005) + tool-output-bound (PERF-004) transform on opencode's messages hook, and
// gets staged to disk. Keeping the context lean is what delays opencode's compaction — which rewrites
// the cached prefix on overflow and forces a cold re-prefill (the observed latency cliff).
func TestContextEfficiencyPluginStaged(t *testing.T) {
	js, ok := pluginSeedFiles[ContextEfficiencyPluginFile]
	if !ok {
		t.Fatalf("plugin %q not in pluginSeedFiles", ContextEfficiencyPluginFile)
	}
	for _, want := range []string{
		"experimental.chat.messages.transform", // opencode's per-call messages hook
		`p.type !== "reasoning"`,               // PERF-005: drop stale reasoning
		"toolInvocation",                       // PERF-004: bound tool results
		"truncated",                            // the truncation marker
	} {
		if !strings.Contains(js, want) {
			t.Errorf("context-efficiency plugin missing %q", want)
		}
	}
	dir := t.TempDir()
	if err := stagePluginSeed(dir); err != nil {
		t.Fatalf("stagePluginSeed: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dir, ContextEfficiencyPluginFile)); err != nil {
		t.Errorf("plugin not staged to disk: %v", err)
	}
}

// TestRenderConfigLoadsPlugin → PERF-004/005: the rendered TUI config references the plugin by absolute
// path so opencode loads it on the serve (non --pure) path.
func TestRenderConfigLoadsPlugin(t *testing.T) {
	if _, ok := ConfigSeedDir(); !ok {
		t.Skip("config seed dir unavailable in this environment")
	}
	out := RenderConfig(config.Config{Endpoint: "http://127.0.0.1:8080", ModelID: "m"}, true)
	if !strings.Contains(out, `"plugin"`) || !strings.Contains(out, ContextEfficiencyPluginFile) {
		t.Errorf("rendered config must reference the context-efficiency plugin; got:\n%s", out)
	}
}
