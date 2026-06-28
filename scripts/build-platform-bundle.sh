#!/usr/bin/env bash
# build-platform-bundle.sh <goos> <goarch> <version> — build the FULL per-platform bundle on a
# NATIVE runner: the aegis binary + OpenCode + llama-server + the platform's pinned rg, then
# assemble the tarball (scripts/build-bundle.sh). The matrix in BOTH
# .github/workflows/bundle-matrix.yml (validation) and release.yml (REL-010 publish) calls this
# one script, so the per-platform build logic lives in a single place (REL-009/010).
#
# Must run on a runner whose native platform IS <goos>/<goarch> — OpenCode + llama-server are
# host-built, so e.g. darwin/arm64 must run on macos-latest. Network is needed for the OpenCode
# build (Bun) + the pinned rg fetch; the air-gap boundary is the enclave, not this build host.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"

goos="${1:?usage: build-platform-bundle.sh <goos> <goarch> <version>}"
goarch="${2:?goarch required}"
version="${3:?version required}"; version="${version#v}"   # accept a v-prefixed tag
dist="${DIST:-dist}"; mkdir -p "$dist"

export CGO_ENABLED=0 GOFLAGS=-mod=vendor GOPROXY=off
echo "build-platform-bundle: aegis $version for $goos/$goarch (native)" >&2
GOOS="$goos" GOARCH="$goarch" go build -trimpath -ldflags "-s -w -X main.version=$version" \
	-o "$dist/aegis-$version-$goos-$goarch" ./cmd/aegis

scripts/build-opencode.sh
scripts/build-llama.sh
scripts/stage-ripgrep.sh "$goos-$goarch"
scripts/build-bundle.sh "$goos" "$goarch" "$version" "$dist"
