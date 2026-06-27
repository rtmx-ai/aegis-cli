#!/usr/bin/env bash
# stage-ripgrep.sh [platform] — stage the pinned, VERIFIED ripgrep `rg` binary into
# deploy/opencode/bin/rg (OC-009 / REL-007 multi-platform).
#
# Why: OpenCode's grep tool resolves `rg` from PATH and otherwise DOWNLOADS ripgrep from
# github at bootstrap — a non-loopback egress that also wedges the run offline. aegis prepends
# this staged rg's dir to the launch PATH (internal/opencode airgapEnv) so OpenCode finds it
# and never reaches the network.
#
# `platform` is <goos>-<goarch> (default: the host) and selects the pin from
# deploy/opencode/RIPGREP_REF. Two modes:
#   SIDE-LOAD (air-gap):  RIPGREP_SRC=<dir holding rg> — verified against the pin, no fetch.
#   DOWNLOAD (build host): fetch the platform's pinned tarball, verify the tarball + the
#                          extracted rg against the pin, install. The enclave never fetches.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"
REF="deploy/opencode/RIPGREP_REF"
OUT="${RIPGREP_OUT:-deploy/opencode/bin/rg}"

detect_platform() {
	if command -v go >/dev/null 2>&1; then echo "$(go env GOOS)-$(go env GOARCH)"; return; fi
	os="$(uname -s | tr 'A-Z' 'a-z')"
	case "$(uname -m)" in x86_64|amd64) a=amd64;; aarch64|arm64) a=arm64;; *) a="$(uname -m)";; esac
	echo "$os-$a"
}
PLATFORM="${1:-$(detect_platform)}"

field() { python3 -c "import json;d=json.load(open('$REF'));p=d.get('platforms',{}).get('$PLATFORM') or {};print(p.get('$1',''))"; }
version="$(python3 -c "import json;print(json.load(open('$REF')).get('version',''))")"
triple="$(field triple)"
want="$(field sha256)"
tarball_want="$(field tarball_sha256)"
[ -n "$triple" ] && [ -n "$want" ] || { echo "stage-ripgrep: no pin for platform '$PLATFORM' in $REF" >&2; exit 1; }

sha_of() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
mkdir -p "$(dirname "$OUT")"

if [ -n "${RIPGREP_SRC:-}" ]; then
	src="$RIPGREP_SRC/rg"
	[ -f "$src" ] || { echo "stage-ripgrep: rg not found under $RIPGREP_SRC" >&2; exit 1; }
	got="$(sha_of "$src")"
	[ "$got" = "$want" ] || { echo "stage-ripgrep: sha256 MISMATCH for $PLATFORM rg — refusing (want $want, got $got)" >&2; exit 1; }
	install -m 0755 "$src" "$OUT"
	echo "stage-ripgrep: staged $OUT for $PLATFORM (side-loaded, sha256 verified)"
	exit 0
fi

asset="ripgrep-$version-$triple"
url="https://github.com/BurntSushi/ripgrep/releases/download/$version/$asset.tar.gz"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
echo "stage-ripgrep: fetching $url" >&2
curl -sSL --max-time 180 -o "$tmp/$asset.tar.gz" "$url"
got_tb="$(sha_of "$tmp/$asset.tar.gz")"
[ "$got_tb" = "$tarball_want" ] || { echo "stage-ripgrep: tarball sha256 MISMATCH for $PLATFORM — refusing (want $tarball_want, got $got_tb)" >&2; exit 1; }
tar -C "$tmp" -xzf "$tmp/$asset.tar.gz"
got="$(sha_of "$tmp/$asset/rg")"
[ "$got" = "$want" ] || { echo "stage-ripgrep: rg sha256 MISMATCH for $PLATFORM — refusing (want $want, got $got)" >&2; exit 1; }
install -m 0755 "$tmp/$asset/rg" "$OUT"
echo "stage-ripgrep: staged $OUT for $PLATFORM (downloaded + verified, ripgrep $version)"
