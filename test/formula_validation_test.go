package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"testing"
)

var emptyOSBlock = regexp.MustCompile(`(?s)on_(macos|linux) do\s*end`)

func writeTarballs(t *testing.T, dir, version string, plats ...string) {
	t.Helper()
	for _, p := range plats {
		if err := os.WriteFile(filepath.Join(dir, "aegis-"+version+"-"+p+".tar.gz"), []byte(p), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

// TestFormulaValidation → REQ-REL-014: the release must never publish an incomplete Homebrew formula.
// A complete build (every supported platform) passes; a PARTIAL build missing a platform — the exact
// v1.9.0 regression where darwin failed and the formula could not load on macOS ("requires at least a
// URL") — FAILS the release; and an empty OS block is rejected. This is the build-time gate.
func TestFormulaValidation(t *testing.T) {
	if _, err := exec.LookPath("bash"); err != nil {
		t.Skip("bash unavailable")
	}
	root := repoRoot(t)
	tmpl := filepath.Join(root, "deploy", "homebrew", "aegis.rb")
	fill := func(dist, out string) error {
		c := exec.Command("bash", "scripts/fill-formula.sh", "1.9.0", dist, tmpl, out)
		c.Dir = root
		return c.Run()
	}

	// Complete build → valid, no empty OS block.
	full := t.TempDir()
	writeTarballs(t, full, "1.9.0", "darwin-arm64", "linux-arm64", "linux-amd64")
	outFull := filepath.Join(t.TempDir(), "full.rb")
	if err := fill(full, outFull); err != nil {
		t.Fatalf("a complete build must produce a valid formula: %v", err)
	}
	if b, _ := os.ReadFile(outFull); emptyOSBlock.Match(b) {
		t.Errorf("complete formula must have no empty OS block:\n%s", b)
	}

	// Partial build (darwin absent — the v1.9.0 regression) → must FAIL, so a formula that can't load
	// on macOS is never published.
	partial := t.TempDir()
	writeTarballs(t, partial, "1.9.0", "linux-arm64", "linux-amd64")
	if err := fill(partial, filepath.Join(t.TempDir(), "partial.rb")); err == nil {
		t.Error("a build missing the darwin platform MUST fail the release (would break brew on macOS)")
	}

	// An empty on_macos block is rejected directly by the validator.
	broken := filepath.Join(t.TempDir(), "broken.rb")
	if err := os.WriteFile(broken, []byte("class A < Formula\n  version \"1.0\"\n  on_macos do\n  end\n  on_linux do\n    url \"x-linux-arm64.tar.gz\"\n    sha256 \"ab\"\n  end\nend\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	v := exec.Command("bash", "scripts/validate-formula.sh", broken)
	v.Dir = root
	if err := v.Run(); err == nil {
		t.Error("validate-formula must FAIL on a formula with an empty on_macos block")
	}
}
