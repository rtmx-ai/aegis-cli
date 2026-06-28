package offline

import (
	"strings"
	"testing"
)

// TestV1ReleaseManifest → REQ-REL-002: a tagged v1.0 ships the full manifest — aegis for the 5
// ship targets, the bundled OpenCode per platform (the native-runner matrix), a CycloneDX SBOM,
// a SHA256SUMS manifest, and an offline minisign signature. Structural guard over the pipeline
// (the live v1.0.0 release is the proof; this keeps the pipeline from regressing).
func TestV1ReleaseManifest(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	for _, tgt := range []string{"linux/amd64", "linux/arm64", "darwin/amd64", "darwin/arm64", "windows/amd64"} {
		if !strings.Contains(rel, tgt) {
			t.Errorf("release.sh must build ship target %s", tgt)
		}
	}
	for _, want := range []string{"gen-sbom", "SHA256SUMS", "minisign -S"} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must produce %s", want)
		}
	}
	// Bundled OpenCode per platform comes from the matrix calling build-platform-bundle.sh, and
	// the release runner installs minisign so the manifest is actually signed.
	wf := readRepoFile(t, ".github/workflows/release.yml")
	for _, want := range []string{"bundle:", "build-platform-bundle.sh", "Install minisign"} {
		if !strings.Contains(wf, want) {
			t.Errorf("release.yml must wire %q", want)
		}
	}
	if bp := readRepoFile(t, "scripts/build-platform-bundle.sh"); !strings.Contains(bp, "build-opencode.sh") {
		t.Error("build-platform-bundle.sh must build OpenCode per platform")
	}
}
