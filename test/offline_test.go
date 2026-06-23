// Package offline holds build/airgap invariant tests that span the whole module.
package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// repoRoot returns the module root (one level above this test/ dir).
func repoRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("cannot resolve caller")
	}
	return filepath.Dir(filepath.Dir(file))
}

// TestOfflineNoThirdPartyDeps models BUILD-001: the binary must build from
// vendored/std-lib-only sources. We assert go.mod declares zero require
// directives, so an offline build needs no network fetch at all.
func TestOfflineNoThirdPartyDeps(t *testing.T) {
	root := repoRoot(t)
	data, err := os.ReadFile(filepath.Join(root, "go.mod"))
	if err != nil {
		t.Fatalf("read go.mod: %v", err)
	}
	if strings.Contains(string(data), "require") {
		t.Fatal("go.mod must have no third-party requires (std-lib only for offline build)")
	}
}

// TestOfflineBuildSucceeds models BUILD-001: building the command with module
// downloads disabled (GOFLAGS=-mod=mod GOPROXY=off) succeeds because there are
// no third-party modules to fetch.
func TestOfflineBuildSucceeds(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping build invocation in -short mode")
	}
	root := repoRoot(t)
	cmd := exec.Command("go", "build", "-o", filepath.Join(t.TempDir(), "aegis"), "./cmd/aegis")
	cmd.Dir = root
	cmd.Env = append(os.Environ(), "GOPROXY=off", "GOFLAGS=-mod=mod")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("offline build failed: %v\n%s", err, out)
	}
}
