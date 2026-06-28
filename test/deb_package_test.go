package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func isExecFile(p string) bool {
	fi, err := os.Stat(p)
	return err == nil && !fi.IsDir() && fi.Mode().Perm()&0o111 != 0
}

// TestDebBundlesHarness → REQ-REL-006: the .deb bundles the whole harness into
// /usr/lib/aegis (not just the bare binary), so `apt install aegis` yields a working
// install. Gated/release-tier: builds a real .deb via scripts/build-deb.sh and inspects it;
// skips unless dpkg-deb + a built aegis + the staged helpers are present.
func TestDebBundlesHarness(t *testing.T) {
	root := repoRoot(t)
	if _, err := exec.LookPath("dpkg-deb"); err != nil {
		t.Skip("dpkg-deb not available")
	}
	aegisBin := filepath.Join(root, "bin", "aegis")
	helpers := []string{
		filepath.Join(root, "deploy", "opencode", "bin", "opencode"),
		filepath.Join(root, "deploy", "opencode", "bin", "rg"),
		filepath.Join(root, "deploy", "llama-server", "bin", "llama-server"),
	}
	if !isExecFile(aegisBin) {
		t.Skip("bin/aegis not built (make build)")
	}
	for _, h := range helpers {
		if !isExecFile(h) {
			t.Skipf("harness helper %s not staged — REL-006 is release-tier (make ci-full)", filepath.Base(h))
		}
	}
	archOut, err := exec.Command("dpkg", "--print-architecture").Output()
	if err != nil {
		t.Skipf("dpkg --print-architecture: %v", err)
	}
	hostArch := strings.TrimSpace(string(archOut))

	dist := t.TempDir()
	cmd := exec.Command("scripts/build-deb.sh", hostArch, aegisBin, dist, "0.0.0-test")
	cmd.Dir = root
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build-deb.sh failed: %v\n%s", err, out)
	}
	deb := filepath.Join(dist, "aegis_0.0.0-test_"+hostArch+".deb")
	out, err := exec.Command("dpkg-deb", "--contents", deb).Output()
	if err != nil {
		t.Fatalf("dpkg-deb --contents: %v", err)
	}
	contents := string(out)
	// The harness BINARIES must be bundled (a packaged aegis can't fetch them); the config
	// seed materializes to the user cache at runtime (REL-006), so it is not required here.
	for _, want := range []string{
		"/usr/bin/aegis", "/usr/lib/aegis/opencode", "/usr/lib/aegis/rg", "/usr/lib/aegis/llama-server",
	} {
		if !strings.Contains(contents, want) {
			t.Errorf(".deb must bundle %s for a working `apt install aegis`\n--- contents ---\n%s", want, contents)
		}
	}
}

// TestBundleTarball → REQ-REL-009: scripts/build-bundle.sh assembles the per-platform bundle
// tarball (bin/aegis + libexec/{opencode,rg,llama-server}), and the bundle-matrix workflow
// drives it + per-platform rg staging over a native-runner matrix. The matrix wiring is
// checked always; the real assembly is gated/release-tier like the .deb test.
func TestBundleTarball(t *testing.T) {
	root := repoRoot(t)
	// The multi-platform matrix drives the shared per-platform build script, which in turn runs
	// the assembler + per-platform rg staging (DRY: one build path for both workflows).
	mx := readRepoFile(t, ".github/workflows/bundle-matrix.yml")
	for _, want := range []string{"matrix:", "scripts/build-platform-bundle.sh"} {
		if !strings.Contains(mx, want) {
			t.Errorf("bundle-matrix.yml must drive %q (REL-009)", want)
		}
	}
	bp := readRepoFile(t, "scripts/build-platform-bundle.sh")
	for _, want := range []string{"build-bundle.sh", "stage-ripgrep.sh"} {
		if !strings.Contains(bp, want) {
			t.Errorf("build-platform-bundle.sh must drive %q (REL-009)", want)
		}
	}
	aegisBin := filepath.Join(root, "bin", "aegis")
	if !isExecFile(aegisBin) || !isExecFile(filepath.Join(root, "deploy", "opencode", "bin", "opencode")) {
		t.Skip("aegis/opencode not built — REL-009 bundle assembly is release-tier (make ci-full)")
	}
	dist := t.TempDir()
	cmd := exec.Command("scripts/build-bundle.sh", "linux", "amd64", "0.0.0-test", dist, aegisBin)
	cmd.Dir = root
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build-bundle.sh: %v\n%s", err, out)
	}
	out, err := exec.Command("tar", "tzf", filepath.Join(dist, "aegis-0.0.0-test-linux-amd64.tar.gz")).Output()
	if err != nil {
		t.Fatalf("tar tzf: %v", err)
	}
	for _, want := range []string{"bin/aegis", "libexec/opencode"} {
		if !strings.Contains(string(out), want) {
			t.Errorf("bundle tarball must contain %s\n%s", want, out)
		}
	}
}
