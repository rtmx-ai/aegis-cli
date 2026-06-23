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

// TestRuntimeBinaryIsStdLibOnly models the airgap invariant for the shipped
// artifact: the aegis binary must depend on NO third-party module. Test-only
// dependencies (e.g. godog for BDD) are allowed and vendored, but must never
// leak into cmd/aegis. We list the binary's transitive deps and reject any that
// resolve to an external module path.
func TestRuntimeBinaryIsStdLibOnly(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping go list invocation in -short mode")
	}
	root := repoRoot(t)
	cmd := exec.Command("go", "list", "-deps", "./cmd/aegis")
	cmd.Dir = root
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("go list -deps failed: %v\n%s", err, out)
	}
	const mod = "github.com/rtmx-ai/aegis-cli"
	for _, dep := range strings.Fields(string(out)) {
		// A third-party module path has a dotted domain in its first element
		// (e.g. "github.com/..."); std-lib import paths do not.
		first, _, _ := strings.Cut(dep, "/")
		if strings.Contains(first, ".") && !strings.HasPrefix(dep, mod) {
			t.Errorf("aegis binary must be std-lib-only, but depends on third-party %q", dep)
		}
	}
}

// TestOfflineBuildSucceeds models BUILD-001: the command builds with module
// downloads disabled, from the vendored tree (GOFLAGS=-mod=vendor GOPROXY=off),
// so the offline/air-gapped build needs no network fetch.
func TestOfflineBuildSucceeds(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping build invocation in -short mode")
	}
	root := repoRoot(t)
	cmd := exec.Command("go", "build", "-o", filepath.Join(t.TempDir(), "aegis"), "./cmd/aegis")
	cmd.Dir = root
	cmd.Env = append(os.Environ(), "GOPROXY=off", "GOFLAGS=-mod=vendor")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("offline vendored build failed: %v\n%s", err, out)
	}
}
