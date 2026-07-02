package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// TestWindowsCrossCompile → REQ-WIN-001: the aegis binary must cross-compile to both Windows arches,
// CGO-free, from the vendored tree — the foundation for Windows support (the bundle + install paths
// build on this). Proves aegis.exe is buildable without a Windows host.
func TestWindowsCrossCompile(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go unavailable")
	}
	root := repoRoot(t)
	for _, arch := range []string{"amd64", "arm64"} {
		out := filepath.Join(t.TempDir(), "aegis-"+arch+".exe")
		cmd := exec.Command("go", "build", "-trimpath", "-o", out, "./cmd/aegis")
		cmd.Dir = root
		cmd.Env = append(os.Environ(), "GOOS=windows", "GOARCH="+arch, "CGO_ENABLED=0", "GOFLAGS=-mod=vendor")
		if o, err := cmd.CombinedOutput(); err != nil {
			t.Errorf("aegis must cross-compile for windows/%s (CGO-free):\n%s", arch, o)
			continue
		}
		if fi, err := os.Stat(out); err != nil || fi.Size() == 0 {
			t.Errorf("windows/%s build produced no binary", arch)
		}
	}
}
