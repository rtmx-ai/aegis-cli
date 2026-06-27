package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// TestRipgrepPinned guards the OC-009 / REL-007 ripgrep pin: deploy/opencode/RIPGREP_REF must
// pin a concrete sha256 (64-char hex) for EVERY shipped platform (linux + macOS, amd64 +
// arm64) — so scripts/stage-ripgrep.sh can stage a verified rg per platform and the egress
// gate launches OpenCode with a real ripgrep on each. Regression guard for the multi-platform
// pin (a missing/placeholder digest would let OpenCode fetch rg from github = egress).
func TestRipgrepPinned(t *testing.T) {
	b, err := os.ReadFile(filepath.Join(repoRoot(t), "deploy", "opencode", "RIPGREP_REF"))
	if err != nil {
		t.Fatalf("read RIPGREP_REF: %v", err)
	}
	var ref struct {
		Version   string `json:"version"`
		Platforms map[string]struct {
			Triple        string `json:"triple"`
			SHA256        string `json:"sha256"`
			TarballSHA256 string `json:"tarball_sha256"`
		} `json:"platforms"`
	}
	if err := json.Unmarshal(b, &ref); err != nil {
		t.Fatalf("RIPGREP_REF malformed: %v", err)
	}
	if ref.Version == "" {
		t.Error("RIPGREP_REF must pin a ripgrep version")
	}
	for _, plat := range []string{"linux-amd64", "linux-arm64", "darwin-amd64", "darwin-arm64"} {
		p, ok := ref.Platforms[plat]
		if !ok {
			t.Errorf("RIPGREP_REF must pin platform %q", plat)
			continue
		}
		if p.Triple == "" {
			t.Errorf("%s: missing rust target triple", plat)
		}
		if !sha256Re.MatchString(p.SHA256) {
			t.Errorf("%s: sha256 must be a concrete 64-hex digest, got %q", plat, p.SHA256)
		}
		if !sha256Re.MatchString(p.TarballSHA256) {
			t.Errorf("%s: tarball_sha256 must be a concrete 64-hex digest, got %q", plat, p.TarballSHA256)
		}
	}
}
