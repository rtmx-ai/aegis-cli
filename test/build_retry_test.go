package offline

import (
	"strings"
	"testing"
)

// TestBuildScriptsRetryClones → REQ-REL-013: the release bundle build must be resilient to transient CI
// runner flakes (the darwin bundle failed a git op on macos-latest in v1.9.0 AND v1.9.4 with no code
// change in the build path). The build scripts must retry the network-dependent steps — git clone/fetch
// and the registry install — so a one-off network error retries instead of sinking the whole release.
func TestBuildScriptsRetryClones(t *testing.T) {
	for _, tc := range []struct {
		path  string
		wants []string
	}{
		{
			"scripts/build-opencode.sh",
			[]string{"retry()", "clone attempt", "retry git -C", "retry bun install"},
		},
		{
			"scripts/build-llama.sh",
			[]string{"retry()", "clone attempt", "retry git -C"},
		},
	} {
		body := readRepoFile(t, tc.path)
		for _, w := range tc.wants {
			if !strings.Contains(body, w) {
				t.Errorf("%s must retry flaky network steps (missing %q) — a runner flake must not sink a release", tc.path, w)
			}
		}
	}
}
