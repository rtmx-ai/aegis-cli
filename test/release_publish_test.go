package offline

import (
	"strings"
	"testing"
)

// TestReleasePublishIdempotent → REQ-REL-015: the release "Publish release" step must be idempotent.
// A re-run/retry after a transient bundle-build flake must UPDATE the existing release, not fail with
// "release already exists" — that failure stranded v1.9.0's rebuilt darwin bottle and skipped the tap
// publish, forcing a manual delete-and-rerun. The step must also not claim platforms the matrix never
// builds (windows / darwin-amd64) in the release notes.
func TestReleasePublishIdempotent(t *testing.T) {
	wf := readRepoFile(t, ".github/workflows/release.yml")
	for _, want := range []string{"gh release view", "gh release upload", "--clobber"} {
		if !strings.Contains(wf, want) {
			t.Errorf("release.yml Publish release must be idempotent — missing %q", want)
		}
	}
	for _, overclaim := range []string{"windows amd64", "macOS Intel"} {
		if strings.Contains(wf, overclaim) {
			t.Errorf("release notes must not claim an unbuilt platform (%q)", overclaim)
		}
	}
}
