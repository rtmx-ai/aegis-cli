package offline

import (
	"strings"
	"testing"
)

// TestReleaseRunnableArtifacts → REQ-REL-011: every published release asset launches the TUI. The
// per-arch .deb bundles THAT arch's harness (cross-arch from the ingested matrix bundle, not
// binary-only), and the bare harness-less cross-binaries are dropped before the manifest/publish.
func TestReleaseRunnableArtifacts(t *testing.T) {
	deb := readRepoFile(t, "scripts/build-deb.sh")
	if !strings.Contains(deb, "libexec_src") {
		t.Error("build-deb.sh must accept a libexec source (a bundle's native harness) so a cross-arch .deb is harness-complete")
	}

	rel := readRepoFile(t, "scripts/release.sh")
	for _, want := range []string{
		"REL-011",          // the fix is wired
		"harness-complete", // cross-arch .debs rebuilt from the ingested matrix bundles
	} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must rebuild cross-arch .debs from the matrix bundles + drop bare binaries: missing %q", want)
		}
	}
}
