#!/usr/bin/env bash
# release.sh — reproducible, offline, signed release with an SBOM.
#
# Implements docs/requirements/release-packaging.md (BUILD-002..006). Produces an
# enclave-transferable artifact set under dist/: static cross-compiled binaries
# for the ship targets, a CycloneDX SBOM, a SHA-256 checksums manifest, and an
# offline detached signature over that manifest.
#
# Air-gap-first by design:
#   - BUILD-006: GOPROXY=off + -mod=vendor + -trimpath — no network, reproducible
#     for a given commit (binaries depend only on the vendored tree + toolchain).
#   - BUILD-005: signatures are OFFLINE detached (minisign/GPG). We deliberately
#     avoid keyless/online signing schemes that need an online CA + transparency
#     log at sign AND verify time, which a closed enclave cannot reach.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

VERSION="$(tr -d ' \n' < VERSION 2>/dev/null || echo dev)"
COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
DIST="${DIST:-dist}"
rm -rf "$DIST"
mkdir -p "$DIST"

# Reproducible/offline build environment (BUILD-006).
export CGO_ENABLED=0 GOPROXY=off GOFLAGS=-mod=vendor
LDFLAGS="-s -w -X main.version=$VERSION -X main.commit=$COMMIT"

# BUILD-002: static cross-compiled matrix for the ship targets — Linux (amd64 +
# arm64), macOS Apple Silicon (arm64) AND Intel (amd64), and Windows (amd64).
TARGETS="linux/amd64 linux/arm64 darwin/amd64 darwin/arm64 windows/amd64"
for t in $TARGETS; do
	os="${t%/*}"; arch="${t#*/}"
	out="$DIST/aegis-$VERSION-$os-$arch"
	[ "$os" = "windows" ] && out="$out.exe"
	echo "release: building $out"
	GOOS="$os" GOARCH="$arch" go build -trimpath -ldflags "$LDFLAGS" -o "$out" ./cmd/aegis
done

# BUILD-008 / REL-006: Debian packages that bundle the harness (scripts/build-deb.sh —
# shared with test::TestDebBundlesHarness). Called below, AFTER the helpers are staged.
build_deb() {
	deb_arch="$1"
	bin="$DIST/aegis-$VERSION-linux-$deb_arch"
	if ! command -v dpkg-deb >/dev/null 2>&1; then
		echo "release: NOTE — dpkg-deb not found; skipping .deb for $deb_arch (CI ubuntu has it)." >&2
		return 0
	fi
	scripts/build-deb.sh "$deb_arch" "$bin" "$DIST" "$VERSION" >/dev/null \
		&& echo "release: built $DIST/aegis_${VERSION}_${deb_arch}.deb"
}

# BUILD-003: CycloneDX SBOM from the vendored module set.
go list -m -json all >"$DIST/modules.json" 2>/dev/null || echo '{}' >"$DIST/modules.json"
python3 scripts/gen-sbom.py "$DIST/modules.json" "$VERSION" >"$DIST/sbom.cdx.json"
rm -f "$DIST/modules.json"

# OC-005 / TUI-005: build OpenCode from PINNED source (air-gap-hardened) and bundle
# it alongside aegis. We build it ourselves (scripts/build-opencode.sh) rather than
# trust an upstream prebuilt binary; the result is covered by the checksums +
# signature. OPENCODE_BIN overrides with a prebuilt path if explicitly provided.
opencode_bin="${OPENCODE_BIN:-deploy/opencode/bin/opencode}"
if [ ! -x "$opencode_bin" ]; then
	scripts/build-opencode.sh || true
fi
if [ -x "$opencode_bin" ]; then
	cp "$opencode_bin" "$DIST/opencode"
	cp deploy/opencode/OPENCODE_REF "$DIST/opencode.VERSION" 2>/dev/null || true
	echo "release: bundled self-built OpenCode (anomalyco/opencode $(tr -d ' \n' <deploy/opencode/OPENCODE_REF 2>/dev/null))"
else
	echo "release: NOTE — OpenCode not built/bundled; run scripts/build-opencode.sh on a Bun-equipped build host." >&2
fi

# BUILD-011: build + bundle the production llama.cpp llama-server from pinned source
# (the full-stack release tier). Built for the build host's platform; covered by
# the checksums + signature alongside aegis + opencode.
llama_bin="deploy/llama-server/bin/llama-server"
if [ ! -x "$llama_bin" ]; then
	scripts/build-llama.sh || true
fi
if [ -x "$llama_bin" ]; then
	cp "$llama_bin" "$DIST/llama-server"
	cp deploy/llama-server/LLAMA_REF "$DIST/llama-server.VERSION" 2>/dev/null || true
	echo "release: bundled self-built llama-server (llama.cpp $(tr -d ' \n' <deploy/llama-server/LLAMA_REF 2>/dev/null))"
else
	echo "release: NOTE — llama-server not built/bundled; run scripts/build-llama.sh on a toolchain-equipped host." >&2
fi

# REL-006: build the .deb packages now that the harness helpers are staged, so the
# host-arch .deb bundles them under /usr/lib/aegis (a working `apt install`, not a bare bin).
build_deb amd64
build_deb arm64

# REL-006/007: the HOST-platform bundle tarball (aegis + libexec helpers) for the Homebrew
# formula, via scripts/build-bundle.sh — shared with the multi-platform bundle-matrix workflow
# (.github/workflows/bundle-matrix.yml), which builds the other platforms on native runners.
host_os="$(go env GOOS)"; host_arch_go="$(go env GOARCH)"
if [ -x "$DIST/aegis-$VERSION-$host_os-$host_arch_go" ] && [ -x deploy/opencode/bin/opencode ]; then
	scripts/build-bundle.sh "$host_os" "$host_arch_go" "$VERSION" "$DIST" >/dev/null \
		&& echo "release: bundled tarball aegis-$VERSION-$host_os-$host_arch_go.tar.gz"
fi

# BUILD-004: SHA-256 checksums manifest over every artifact (aegis binaries,
# .exe, .deb, the bundled opencode if present, and the SBOM).
(
	cd "$DIST"
	files="$(ls | grep -v '^SHA256SUMS$')"
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum $files >SHA256SUMS
	else
		shasum -a 256 $files >SHA256SUMS
	fi
)

# BUILD-005: offline detached signature over the manifest (air-gap-first).
if command -v minisign >/dev/null 2>&1 && [ -n "${MINISIGN_KEY:-}" ]; then
	minisign -S -s "$MINISIGN_KEY" -m "$DIST/SHA256SUMS"
	echo "release: signed SHA256SUMS with minisign (detached, offline-verifiable)"
elif command -v gpg >/dev/null 2>&1 && [ -n "${GPG_KEY:-}" ]; then
	gpg --batch --yes --local-user "$GPG_KEY" --armor --detach-sign "$DIST/SHA256SUMS"
	echo "release: signed SHA256SUMS with gpg (detached, offline-verifiable)"
else
	echo "release: NOTE — no signing key (set MINISIGN_KEY or GPG_KEY); SHA256SUMS is UNSIGNED." >&2
	echo "release: a real release REQUIRES an offline detached signature (minisign/gpg)." >&2
fi

echo "release: $VERSION ($COMMIT) — $(ls "$DIST"/aegis-* | wc -l | tr -d ' ') binaries + SBOM + checksums in $DIST/"
