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

// TestOpenCodePinned → REQ-OC-001: a pinned anomalyco/opencode source ref.
func TestOpenCodePinned(t *testing.T) {
	ref := strings.TrimSpace(readRepoFile(t, "deploy/opencode/OPENCODE_REF"))
	if !strings.HasPrefix(ref, "v") || len(ref) < 4 {
		t.Errorf("OPENCODE_REF must pin a concrete version, got %q", ref)
	}
	if !strings.Contains(readRepoFile(t, "scripts/build-opencode.sh"), "anomalyco/opencode") {
		t.Error("the build must source anomalyco/opencode")
	}
}

// TestBuildOpenCodeConfigured → REQ-OC-002: a hardened build from pinned source.
func TestBuildOpenCodeConfigured(t *testing.T) {
	b := readRepoFile(t, "scripts/build-opencode.sh")
	for _, want := range []string{"OPENCODE_REF", "script/build.ts", "--single",
		"OPENCODE_TELEMETRY=0", "OPENCODE_AUTOUPDATE=0", "OPENCODE_DISABLE_SHARE=1"} {
		if !strings.Contains(b, want) {
			t.Errorf("build-opencode.sh must configure %q", want)
		}
	}
}

// TestOpenCodeBuildIsOfflineHardened → REQ-OC-003: offline frozen deps; the build
// degrades safely without Bun, and the binary is bundled under the egress gate.
func TestOpenCodeBuildIsOfflineHardened(t *testing.T) {
	b := readRepoFile(t, "scripts/build-opencode.sh")
	if !strings.Contains(b, "--frozen-lockfile") {
		t.Error("build must install dependencies offline (--frozen-lockfile)")
	}
	if !strings.Contains(b, "command -v bun") {
		t.Error("build must degrade cleanly when bun is absent (gated host step)")
	}
}

// TestReleaseBuildsOpenCode → REQ-OC-005: the release builds OpenCode from source
// and bundles it with provenance.
func TestReleaseBuildsOpenCode(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	for _, want := range []string{"build-opencode.sh", "opencode", "OPENCODE_REF"} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must build + bundle OpenCode (%q)", want)
		}
	}
}

// TestOpenCodePinTracksStable → REQ-OC-008: the pin tracks the latest STABLE
// upstream release, surfaced by a check helper (deliberate bumps, never floating).
func TestOpenCodePinTracksStable(t *testing.T) {
	chk := readRepoFile(t, "scripts/check-opencode-latest.sh")
	for _, want := range []string{"OPENCODE_REF", "prerelease", "anomalyco/opencode"} {
		if !strings.Contains(chk, want) {
			t.Errorf("check-opencode-latest.sh must reference %q", want)
		}
	}
}

// TestLlamaServerBuildConfigured → REQ-SERVE-018: the production llama.cpp build
// is pinned, air-gapped (no libcurl), and target-aware.
func TestLlamaServerBuildConfigured(t *testing.T) {
	b := readRepoFile(t, "scripts/build-llama.sh")
	for _, want := range []string{"LLAMA_REF", "LLAMA_CURL=OFF", "llama-server", "GGML_METAL", "GGML_NATIVE"} {
		if !strings.Contains(b, want) {
			t.Errorf("build-llama.sh must configure %q", want)
		}
	}
	ref := strings.TrimSpace(readRepoFile(t, "deploy/llama-server/LLAMA_REF"))
	if ref == "" || ref == "master" {
		t.Errorf("LLAMA_REF must pin a concrete release tag, got %q", ref)
	}
}

// TestModelPinned → REQ-MODEL-001: the model GGUF is pinned (name + sha256).
func TestModelPinned(t *testing.T) {
	ref := readRepoFile(t, "deploy/models/MODEL_REF")
	for _, want := range []string{".gguf", "sha256"} {
		if !strings.Contains(ref, want) {
			t.Errorf("MODEL_REF must pin a GGUF by %q", want)
		}
	}
}

// TestStageModelConfigured → REQ-MODEL-002: stage-model.sh verifies the pin.
func TestStageModelConfigured(t *testing.T) {
	s := readRepoFile(t, "scripts/stage-model.sh")
	for _, want := range []string{"MODEL_REF", "sha256sum", "MISMATCH"} {
		if !strings.Contains(s, want) {
			t.Errorf("stage-model.sh must verify the pin (%q)", want)
		}
	}
}

// TestCIFullTarget → REQ-BUILD-010: `make ci-full` builds the whole stack.
func TestCIFullTarget(t *testing.T) {
	mk := readRepoFile(t, "Makefile")
	if !strings.Contains(mk, "ci-full:") {
		t.Error("Makefile must define ci-full")
	}
	for _, want := range []string{"build-opencode.sh", "build-llama.sh"} {
		if !strings.Contains(mk, want) {
			t.Errorf("ci-full must build %q", want)
		}
	}
}

// TestReleaseBuildsFullStack → REQ-BUILD-011: the release builds the full stack.
func TestReleaseBuildsFullStack(t *testing.T) {
	rel := readRepoFile(t, "scripts/release.sh")
	for _, want := range []string{"build-opencode.sh", "build-llama.sh", "llama-server"} {
		if !strings.Contains(rel, want) {
			t.Errorf("release.sh must build the full stack (%q)", want)
		}
	}
}
