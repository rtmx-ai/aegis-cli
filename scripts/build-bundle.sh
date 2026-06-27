#!/usr/bin/env bash
# build-bundle.sh <os> <arch> <version> <dist-dir> [aegis-bin] — assemble the release/Homebrew
# bundle tarball  dist/aegis-<version>-<os>-<arch>.tar.gz  containing:
#   bin/aegis
#   libexec/{opencode, rg, llama-server}   (the libexec layout aegis resolves, REL-005/006)
# The config seed materializes to the user cache at runtime (REL-006), so it is NOT bundled.
#
# The helpers (OpenCode, ripgrep, llama-server) are the BUILD HOST's — so this must run on a
# NATIVE runner per platform (REL-007 multi-platform; see .github/workflows/bundle-matrix.yml).
# Shared by scripts/release.sh (host platform) + the bundle-matrix workflow (all platforms).
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"

os="${1:?usage: build-bundle.sh <os> <arch> <version> <dist-dir> [aegis-bin]}"
arch="${2:?arch required}"
version="${3:?version required}"
dist="${4:?dist dir required}"
aegis="${5:-$dist/aegis-$version-$os-$arch}"

[ -x "$aegis" ] || { echo "build-bundle: aegis binary not found/executable: $aegis" >&2; exit 1; }
[ -x deploy/opencode/bin/opencode ] || { echo "build-bundle: OpenCode not built (scripts/build-opencode.sh)" >&2; exit 1; }
mkdir -p "$dist"

b="$(mktemp -d)"; trap 'rm -rf "$b"' EXIT
mkdir -p "$b/bin" "$b/libexec"
install -m 0755 "$aegis" "$b/bin/aegis"
install -m 0755 deploy/opencode/bin/opencode "$b/libexec/opencode"
[ -x deploy/opencode/bin/rg ] && install -m 0755 deploy/opencode/bin/rg "$b/libexec/rg"
[ -x deploy/llama-server/bin/llama-server ] && install -m 0755 deploy/llama-server/bin/llama-server "$b/libexec/llama-server"

out="$dist/aegis-$version-$os-$arch.tar.gz"
tar -C "$b" -czf "$out" bin libexec
echo "build-bundle: $out [$(ls "$b/libexec" | tr '\n' ' ')]" >&2
echo "$out"
