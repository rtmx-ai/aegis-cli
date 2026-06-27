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

	// The hardened launch env must point OPENCODE_CONFIG_DIR at the seed so OpenCode
	// installs there (a no-op) instead of fetching from registry.npmjs.org.
	var got string
	for _, e := range airgapEnv(config.Default(), true) {
		if strings.HasPrefix(e, "OPENCODE_CONFIG_DIR=") {
			got = strings.TrimPrefix(e, "OPENCODE_CONFIG_DIR=")
		}
	}
	if got != seed {
		t.Errorf("airgapEnv OPENCODE_CONFIG_DIR = %q, want seed dir %q", got, seed)
	}

	// Idempotent: a second stage over the same dir must not error or duplicate.
	if err := stagePluginSeed(seed); err != nil {
		t.Errorf("stagePluginSeed not idempotent: %v", err)
	}
}
