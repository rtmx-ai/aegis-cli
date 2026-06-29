package offline

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestHarnessProvisionInteractive → REQ-OC-026: the no-model provisioning screen lets the operator
// trigger `aegis provision` with a keypress, streams its progress inline, and transitions to the
// prompt on success — wired into the OC-017 home.tsx patch and baked into the binary. (Patch +
// binary structural guard; the live keypress→download→prompt flow is confirmed by driving.)
func TestHarnessProvisionInteractive(t *testing.T) {
	patch := readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
	for _, want := range []string{
		"AEGIS_BIN",  // the screen spawns the aegis binary...
		"provision",  // ...running `aegis provision`
		"downloaded", // parses the engine's "downloaded X/Y GB" progress lines
		"spawn",      // via a child-process spawn
	} {
		if !strings.Contains(patch, want) {
			t.Errorf("the patch must wire interactive provisioning: missing %q", want)
		}
	}
	bin := filepath.Join(repoRoot(t), "deploy", "opencode", "bin", "opencode")
	if fi, err := os.Stat(bin); err == nil && fi.Mode().Perm()&0o111 != 0 {
		if grepBinaryCount(bin, "AEGIS_BIN") == 0 {
			t.Error("built opencode must carry interactive provisioning (the AEGIS_BIN spawn)")
		}
	}
}
