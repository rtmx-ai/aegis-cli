package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// TestTapFormulaPinned → REQ-REL-007 (brew channel): the Homebrew formula template carries the
// per-platform placeholders + the libexec install (REL-005/006), and scripts/fill-formula.sh
// pins it to a released version + a concrete sha256 from the bundle tarball — what the tap
// (brew install rtmx-ai/tap/aegis) serves.
func TestTapFormulaPinned(t *testing.T) {
	root := repoRoot(t)
	tmpl := filepath.Join(root, "deploy", "homebrew", "aegis.rb")
	src, err := os.ReadFile(tmpl)
	if err != nil {
		t.Fatalf("formula template missing: %v", err)
	}
	for _, want := range []string{
		"REPLACE_LINUX_AMD64_SHA256", "REPLACE_DARWIN_ARM64_SHA256",
		"libexec.install", "AEGIS_LIBEXEC", "write_env_script",
	} {
		if !strings.Contains(string(src), want) {
			t.Errorf("formula template must contain %q", want)
		}
	}
	if _, err := exec.LookPath("bash"); err != nil {
		t.Skip("bash unavailable")
	}
	dist := t.TempDir()
	// All supported platforms must be present or fill-formula fails (REL-014, complete release).
	for _, p := range []string{"darwin-arm64", "linux-arm64", "linux-amd64"} {
		if err := os.WriteFile(filepath.Join(dist, "aegis-1.2.3-"+p+".tar.gz"), []byte(p), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	out := filepath.Join(t.TempDir(), "aegis.rb")
	cmd := exec.Command("bash", "scripts/fill-formula.sh", "1.2.3", dist, tmpl, out)
	cmd.Dir = root
	if o, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("fill-formula: %v\n%s", err, o)
	}
	filled, _ := os.ReadFile(out)
	if !strings.Contains(string(filled), `version "1.2.3"`) {
		t.Error("filled formula must pin the version")
	}
	if !regexp.MustCompile(`sha256 "[0-9a-f]{64}"`).MatchString(string(filled)) {
		t.Errorf("filled formula must pin a concrete sha256:\n%s", filled)
	}
	// No placeholder may survive — unbuilt platforms are pruned so the published formula is
	// valid (a REPLACE_ sha256 breaks brew on that platform).
	if strings.Contains(string(filled), "REPLACE_") {
		t.Errorf("no REPLACE_ placeholder may survive (unbuilt platforms must be pruned):\n%s", filled)
	}
	if !strings.Contains(string(filled), "aegis-#{version}-linux-amd64.tar.gz") {
		t.Error("the built platform (linux-amd64) must remain in the formula")
	}
}
