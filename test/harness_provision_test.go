package offline

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestHarnessProvisionScreen → REQ-OC-022: when AEGIS_NO_MODEL is set, the opencode home screen
// renders an in-app model-selection/provisioning screen (best-fit + how to provision via
// `aegis provision`) instead of dropping the operator to a dead prompt — wired by the OC-017 patch
// over home.tsx and baked into the built binary.
func TestHarnessProvisionScreen(t *testing.T) {
	root := repoRoot(t)
	patch := readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
	for _, want := range []string{"AEGIS_NO_MODEL", "aegis provision"} {
		if !strings.Contains(patch, want) {
			t.Errorf("the patch must wire the no-model provisioning screen: missing %q", want)
		}
	}
	bin := filepath.Join(root, "deploy", "opencode", "bin", "opencode")
	if fi, err := os.Stat(bin); err == nil && fi.Mode().Perm()&0o111 != 0 {
		for _, sig := range []string{"aegis provision", "No model"} {
			if grepBinaryCount(bin, sig) == 0 {
				t.Errorf("built opencode must carry the provisioning screen: missing %q", sig)
			}
		}
	}
}
