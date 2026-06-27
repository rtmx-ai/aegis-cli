package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"testing"
)

// TestRipgrepPinned guards the OC-009 ripgrep pin: deploy/opencode/RIPGREP_REF must
// carry a concrete sha256 (a 64-char hex), not the PENDING placeholder — so
// scripts/stage-ripgrep.sh stages a verified rg (and the egress gate launches
// OpenCode with a real ripgrep) instead of refusing. Regression guard for the pin.
func TestRipgrepPinned(t *testing.T) {
	b, err := os.ReadFile(filepath.Join(repoRoot(t), "deploy", "opencode", "RIPGREP_REF"))
	if err != nil {
		t.Fatalf("read RIPGREP_REF: %v", err)
	}
	var ref struct {
		Version string `json:"version"`
		SHA256  string `json:"sha256"`
	}
	if err := json.Unmarshal(b, &ref); err != nil {
		t.Fatalf("RIPGREP_REF malformed: %v", err)
	}
	if ref.Version == "" {
		t.Error("RIPGREP_REF must pin a ripgrep version")
	}
	if !regexp.MustCompile(`^[0-9a-f]{64}$`).MatchString(ref.SHA256) {
		t.Errorf("RIPGREP_REF sha256 must be a concrete 64-hex digest (not PENDING), got %q", ref.SHA256)
	}
}
