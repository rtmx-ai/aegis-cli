package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestPluginInstallSuppressed is the OC-010 acceptance test: aegis materializes a
// seed that satisfies OpenCode's "@opencode-ai/plugin already installed" check, and
// points OPENCODE_CONFIG_DIR at it, so bootstrap performs no npm-registry install.
func TestPluginInstallSuppressed(t *testing.T) {
	t.Chdir(t.TempDir())

	seed, ok := ConfigSeedDir()
	if !ok {
		t.Fatal("ConfigSeedDir failed to stage the plugin seed")
	}
	t.Cleanup(func() { _ = os.RemoveAll(seed) })

	// The seed must satisfy OpenCode's skip condition: a node_modules entry for the
	// package AND a lockfile that lists it => bootstrap skips the npm install.
	if _, err := os.Stat(filepath.Join(seed, "node_modules", "@opencode-ai", "plugin", "package.json")); err != nil {
		t.Errorf("seed missing node_modules/@opencode-ai/plugin: %v", err)
	}
	lock, err := os.ReadFile(filepath.Join(seed, "package-lock.json"))
	if err != nil {
		t.Fatalf("reading seed package-lock.json: %v", err)
	}
	if !strings.Contains(string(lock), "@opencode-ai/plugin") {
		t.Error("seed package-lock.json does not list @opencode-ai/plugin")
	}

	// The seed must BE OpenCode's config dir: its Global.Path.config is derived as
	// XDG_CONFIG_HOME/opencode, so the seed must be named "opencode" and the hardened
	// env must point XDG_CONFIG_HOME at its parent. (OPENCODE_CONFIG_DIR does not
	// redirect that path — the OC-010 gap that left ~/.config/opencode unseeded and
	// reaching the npm registry.)
	if filepath.Base(seed) != "opencode" {
		t.Errorf("seed dir must be named opencode (OpenCode's XDG config subdir); got %q", seed)
	}
	var xdg string
	for _, e := range airgapEnv(config.Default(), true) {
		if strings.HasPrefix(e, "XDG_CONFIG_HOME=") {
			xdg = strings.TrimPrefix(e, "XDG_CONFIG_HOME=")
		}
	}
	if xdg != filepath.Dir(seed) {
		t.Errorf("airgapEnv XDG_CONFIG_HOME = %q, want seed parent %q (so XDG_CONFIG_HOME/opencode == seed)", xdg, filepath.Dir(seed))
	}

	// Idempotent: a second stage over the same dir must not error or duplicate.
	if err := stagePluginSeed(seed); err != nil {
		t.Errorf("stagePluginSeed not idempotent: %v", err)
	}
}
