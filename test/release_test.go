package offline

import (
	"strings"
	"testing"
)

// TestReleaseBuildMatrixConfigured → REQ-BUILD-002: static cross-compiled matrix
// for the ship targets, stamped + trimmed.
func TestReleaseBuildMatrixConfigured(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	for _, want := range []string{"CGO_ENABLED=0", "-trimpath", "main.version", "linux/amd64", "linux/arm64", "darwin/amd64", "darwin/arm64", "windows/amd64"} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must configure %q", want)
		}
	}
}

// TestSBOMGenerationConfigured → REQ-BUILD-003: a CycloneDX SBOM is produced.
func TestSBOMGenerationConfigured(t *testing.T) {
	if !strings.Contains(readRepoFile(t, "scripts/release.sh"), "gen-sbom.py") {
		t.Error("release.sh must generate an SBOM")
	}
	gen := readRepoFile(t, "scripts/gen-sbom.py")
	for _, want := range []string{"CycloneDX", "bomFormat", "pkg:golang/"} {
		if !strings.Contains(gen, want) {
			t.Errorf("gen-sbom.py must emit %q", want)
		}
	}
}

// TestChecksumsManifestConfigured → REQ-BUILD-004: a SHA-256 manifest covers
// every artifact.
func TestChecksumsManifestConfigured(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	if !strings.Contains(rel, "SHA256SUMS") || !(strings.Contains(rel, "sha256sum") || strings.Contains(rel, "shasum")) {
		t.Error("release.sh must produce a SHA256SUMS manifest")
	}
}

// TestReleaseSigningConfigured → REQ-BUILD-005: offline detached signatures, not
// keyless.
func TestReleaseSigningConfigured(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	if !strings.Contains(rel, "minisign") || !strings.Contains(rel, "detach-sign") {
		t.Error("release.sh must sign with offline detached minisign/gpg")
	}
	if !strings.Contains(rel, "SHA256SUMS") {
		t.Error("signature must cover the checksums manifest")
	}
	// Air-gap-first: must NOT depend on keyless/online signing.
	if strings.Contains(rel, "cosign") {
		t.Error("release must not use keyless cosign (needs online CA/transparency log)")
	}
}

// TestReleaseIsOfflineReproducible → REQ-BUILD-006: offline/vendored + reproducible flags.
func TestReleaseIsOfflineReproducible(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	for _, want := range []string{"GOPROXY=off", "-mod=vendor", "-trimpath"} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must build offline/reproducibly (%q)", want)
		}
	}
}

// TestReleaseWorkflowConfigured → REQ-BUILD-007: a tag-triggered workflow runs
// make release.
func TestReleaseWorkflowConfigured(t *testing.T) {
	wf := readRepoFile(t, ".github/workflows/release.yml")
	for _, want := range []string{"tags:", "make release", "SHA256SUMS", "sbom.cdx.json"} {
		if !strings.Contains(wf, want) {
			t.Errorf("release workflow must configure %q", want)
		}
	}
}

// TestDebianPackagingConfigured → REQ-BUILD-008: release builds .deb packages
// for the Linux targets.
func TestDebianPackagingConfigured(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	for _, want := range []string{"dpkg-deb", "build_deb", "amd64", "arm64", "Architecture:"} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must build .deb packages (%q)", want)
		}
	}
}

// TestReleaseSigningVerifiable models REQ-BUILD-009: a documented offline
// sign/verify procedure plus `make verify-release` (signature + checksums).
func TestReleaseSigningVerifiable(t *testing.T) {
	mk := readRepoFile(t, "Makefile")
	if !strings.Contains(mk, "verify-release:") {
		t.Error("Makefile must define a verify-release target")
	}
	for _, want := range []string{"SHA256SUMS", "minisign", "gpg --verify"} {
		if !strings.Contains(mk, want) {
			t.Errorf("verify-release must check %q", want)
		}
	}
	doc := readRepoFile(t, "docs/release-signing.md")
	for _, topic := range []string{"minisign", "detached", "verify", "offline"} {
		if !strings.Contains(doc, topic) {
			t.Errorf("release-signing doc must cover %q", topic)
		}
	}
	if !strings.Contains(readRepoFile(t, "deploy/release/README.md"), "public") {
		t.Error("deploy/release must document the public-key location")
	}
}

// TestReleaseBundlesOpenCode models REQ-TUI-005: the release bundles the OpenCode
// binary for offline distribution.
func TestReleaseBundlesOpenCode(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	for _, want := range []string{"OPENCODE_BIN", "opencode"} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must bundle OpenCode (%q)", want)
		}
	}
}
