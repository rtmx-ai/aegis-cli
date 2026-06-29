package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

func repoRootOC(t *testing.T) string {
	t.Helper()
	d, _ := os.Getwd()
	for i := 0; i < 8; i++ {
		if _, err := os.Stat(filepath.Join(d, "go.mod")); err == nil {
			return d
		}
		d = filepath.Dir(d)
	}
	t.Fatal("repo root not found")
	return ""
}

// TestRtmxBundledConfigured → REQ-OC-019: rtmx is bundled and the TUI ships the rtmx MCP
// configured out of the box. The hardened config wires the rtmx MCP only when intent is on (the
// default TUI launch uses airgapEnv(cfg, true)), and the bundle + .deb ship rtmx into libexec —
// where hardenedPath puts it on the launch PATH (OC-019), so the MCP command resolves it with no
// operator install.
func TestRtmxBundledConfigured(t *testing.T) {
	cfg := config.Config{Endpoint: "http://127.0.0.1:8080"}
	if rc := RenderConfig(cfg, true); !strings.Contains(rc, `"rtmx"`) || !strings.Contains(rc, "mcp-server") {
		t.Error("RenderConfig(intent=true) must wire the rtmx MCP (next/claim/verify/set_status)")
	}
	if strings.Contains(RenderConfig(cfg, false), `"rtmx"`) {
		t.Error("RenderConfig(intent=false) must omit the rtmx MCP")
	}
	root := repoRootOC(t)
	for _, f := range []string{"scripts/build-bundle.sh", "scripts/build-deb.sh"} {
		b, err := os.ReadFile(filepath.Join(root, f))
		if err != nil || !strings.Contains(string(b), "deploy/rtmx/bin/rtmx") {
			t.Errorf("%s must bundle rtmx into libexec for the out-of-the-box MCP", f)
		}
	}
	// hardenedPath must be wired to put the bundled rtmx dir on the launch PATH (the resolver).
	if !strings.Contains(readFileOC(t, root, "internal/opencode/ripgrep.go"), "ResolveRtmx()") {
		t.Error("hardenedPath must prepend the bundled rtmx dir (ResolveRtmx) to the launch PATH")
	}
}

func readFileOC(t *testing.T, root, rel string) string {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(root, rel))
	if err != nil {
		t.Fatalf("read %s: %v", rel, err)
	}
	return string(b)
}
