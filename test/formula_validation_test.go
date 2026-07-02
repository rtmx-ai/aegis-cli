package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"testing"
)

var emptyOSBlock = regexp.MustCompile(`(?s)on_(macos|linux) do\s*end`)

// TestFormulaValidation → REQ-REL-014: the exact v1.9.0 regression — when a platform's tarball is
// absent (darwin failed to build), fill-formula must NOT leave an empty on_macos/on_linux block
// (Homebrew rejects it: "formula requires at least a URL", which broke `brew upgrade` for the whole
// tap), and validate-formula must FAIL such a formula so a broken one is never published.
func TestFormulaValidation(t *testing.T) {
	if _, err := exec.LookPath("bash"); err != nil {
		t.Skip("bash unavailable")
	}
	root := repoRoot(t)
	tmpl := filepath.Join(root, "deploy", "homebrew", "aegis.rb")

	// Reproduce v1.9.0: only the linux tarballs built; both darwin tarballs absent.
	dist := t.TempDir()
	for _, p := range []string{"linux-amd64", "linux-arm64"} {
		if err := os.WriteFile(filepath.Join(dist, "aegis-1.9.0-"+p+".tar.gz"), []byte(p), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	out := filepath.Join(t.TempDir(), "aegis.rb")
	cmd := exec.Command("bash", "scripts/fill-formula.sh", "1.9.0", dist, tmpl, out)
	cmd.Dir = root
	if o, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("fill-formula (darwin absent) must produce a VALID formula, got error: %v\n%s", err, o)
	}
	filled, _ := os.ReadFile(out)
	if emptyOSBlock.Match(filled) {
		t.Errorf("fill-formula left an EMPTY on_macos/on_linux block (the v1.9.0 brew breakage):\n%s", filled)
	}

	// validate-formula must REJECT a formula with an empty OS block...
	broken := filepath.Join(t.TempDir(), "broken.rb")
	if err := os.WriteFile(broken, []byte("class A < Formula\n  version \"1.0\"\n  on_macos do\n  end\n  on_linux do\n    url \"http://x\"\n    sha256 \"ab\"\n  end\nend\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	v := exec.Command("bash", "scripts/validate-formula.sh", broken)
	v.Dir = root
	if err := v.Run(); err == nil {
		t.Error("validate-formula must FAIL on a formula with an empty on_macos block")
	}

	// ...and ACCEPT the (valid, linux-only) generated formula.
	v2 := exec.Command("bash", "scripts/validate-formula.sh", out)
	v2.Dir = root
	if err := v2.Run(); err != nil {
		t.Errorf("validate-formula must ACCEPT the valid generated formula: %v", err)
	}
}
