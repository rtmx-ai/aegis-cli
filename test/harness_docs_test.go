package offline

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestHarnessDocsAegis → REQ-OC-015: the in-binary docs/help reference aegis, not opencode.ai —
// the docs command + every model prompt's self-identity are rebranded (via the OC-017 patch),
// and no opencode.ai/docs is surfaced to the operator.
func TestHarnessDocsAegis(t *testing.T) {
	root := repoRoot(t)
	patch := readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
	for _, want := range []string{"You are aegis", "rtmx-ai/aegis-cli/tree/main/docs"} {
		if !strings.Contains(patch, want) {
			t.Errorf("rebrand patch must point docs/help at aegis: missing %q", want)
		}
	}
	bin := filepath.Join(root, "deploy", "opencode", "bin", "opencode")
	if fi, err := os.Stat(bin); err == nil && fi.Mode().Perm()&0o111 != 0 {
		if grepBinaryCount(bin, "opencode.ai/docs") != 0 {
			t.Error("built opencode still surfaces opencode.ai/docs to the operator")
		}
		if grepBinaryCount(bin, "You are aegis") == 0 {
			t.Error("built opencode's model prompt must identify as aegis")
		}
	}
}
