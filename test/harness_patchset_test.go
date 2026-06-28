package offline

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestHarnessPatchSet → REQ-OC-017: build-opencode.sh applies aegis's build-time hardening +
// rebranding patches over the PINNED upstream source (deploy/opencode/patches/*.patch) after
// checkout, fail-loud on a conflict (so an OC-008 bump can't silently drop a control) — a patch
// set, not a fork.
func TestHarnessPatchSet(t *testing.T) {
	root := repoRoot(t)
	sh := readRepoFile(t, "scripts/build-opencode.sh")
	for _, want := range []string{
		"deploy/opencode/patches", // the patch dir
		`git -C "$SRC" apply`,     // applies each patch to the pinned checkout
		"apply --check",           // fail-loud pre-check on a bump conflict
		`reset --hard`,            // pristine source -> idempotent apply across re-runs
	} {
		if !strings.Contains(sh, want) {
			t.Errorf("build-opencode.sh must apply the OC-017 patch set (missing %q)", want)
		}
	}
	if fi, err := os.Stat(filepath.Join(root, "deploy", "opencode", "patches")); err != nil || !fi.IsDir() {
		t.Errorf("deploy/opencode/patches/ must exist for the patch set: %v", err)
	}
}
