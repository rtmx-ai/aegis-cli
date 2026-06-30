package offline

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestHarnessVersionFooter → REQ-OC-030: the TUI footer is rebranded to aegis and shows the aegis
// version (AEGIS_VERSION), not OpenCode + the opencode build version.
func TestHarnessVersionFooter(t *testing.T) {
	patch := readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
	if !strings.Contains(patch, "AEGIS_VERSION") {
		t.Error("the footer must read the aegis version via AEGIS_VERSION")
	}
	bin := filepath.Join(repoRoot(t), "deploy", "opencode", "bin", "opencode")
	if fi, err := os.Stat(bin); err == nil && fi.Mode().Perm()&0o111 != 0 {
		if grepBinaryCount(bin, "AEGIS_VERSION") == 0 {
			t.Error("built opencode must carry the AEGIS_VERSION footer")
		}
	}
}
