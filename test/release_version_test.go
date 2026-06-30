package offline

import (
	"strings"
	"testing"
)

// TestReleaseUsesTagVersion → REQ-REL-012: in a tag release, scripts/release.sh keys the version off
// the tag (GITHUB_REF_NAME) when it diverges from the VERSION file — a stale VERSION file shipped a
// broken, URL-less Homebrew formula (v1.3.7).
func TestReleaseUsesTagVersion(t *testing.T) {
	sh := readRepoFile(t, "scripts/release.sh")
	if !strings.Contains(sh, "GITHUB_REF_NAME") {
		t.Fatal("release.sh must key the release version off the tag (GITHUB_REF_NAME)")
	}
	if !strings.Contains(sh, `VERSION="${GITHUB_REF_NAME#v}"`) {
		t.Error("release.sh must override VERSION with the tag on a mismatch, so assets + formula agree")
	}
}
