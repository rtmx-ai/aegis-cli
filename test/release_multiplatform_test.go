package offline

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestReleaseMultiplatform → REQ-REL-010: the tag-triggered release.yml wires the per-platform
// bundle matrix, has the release job consume the bundle artifacts, and release.sh ingests them
// into the signed + published set so fill-formula pins every built platform. This is a
// STRUCTURAL guard — it proves the wiring, not a live publish (that needs the matrix validated
// on real runners + a signed tag), so REL-010 stays MISSING until that happens.
func TestReleaseMultiplatform(t *testing.T) {
	rel := readRepoFile(t, ".github/workflows/release.yml")
	for _, want := range []string{
		"bundle:",                          // the per-platform matrix job
		"scripts/build-platform-bundle.sh", // shared per-platform build
		"bundle-${{ matrix.goos }}-${{ matrix.goarch }}", // artifact naming
		"needs: bundle",     // release consumes the matrix
		"download-artifact", // ...by downloading the tarballs
	} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.yml must wire the bundle matrix — missing %q", want)
		}
	}
	// release.sh ingests the downloaded matrix bundles into dist/ before checksums, so they are
	// signed + published (it wipes dist/ at the top, hence a separate BUNDLES_DIR).
	if sh := readRepoFile(t, "scripts/release.sh"); !strings.Contains(sh, "BUNDLES_DIR") {
		t.Error("release.sh must ingest matrix bundles (BUNDLES_DIR) into dist/ before SHA256SUMS")
	}
	if _, err := os.Stat(filepath.Join(repoRoot(t), "scripts", "build-platform-bundle.sh")); err != nil {
		t.Errorf("scripts/build-platform-bundle.sh (shared per-platform build) missing: %v", err)
	}
}
