#!/usr/bin/env bash
# build-deb.sh <arch> <aegis-binary> <out-dir> [version] — build a Debian package that
# bundles the whole harness (REL-006): /usr/bin/aegis plus, for the build HOST's arch,
# /usr/lib/aegis/{opencode,rg,oc-config,llama-server} (the libexec layout aegis resolves,
# REL-005). So `apt install aegis` is a WORKING install, not a bare binary. A cross-arch
# .deb stays binary-only (the helpers are host-built; cross-built helpers are future work).
# Used by scripts/release.sh and test::TestDebBundlesHarness.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"

arch="${1:?usage: build-deb.sh <arch> <aegis-binary> <out-dir> [version]}"
bin="${2:?aegis binary required}"
out="${3:?out dir required}"
VERSION="${4:-$(tr -d ' \n' < VERSION 2>/dev/null || echo dev)}"

command -v dpkg-deb >/dev/null 2>&1 || { echo "build-deb: dpkg-deb not found" >&2; exit 2; }
[ -x "$bin" ] || { echo "build-deb: aegis binary not executable: $bin" >&2; exit 1; }
mkdir -p "$out"

root="$(mktemp -d)"; trap 'rm -rf "$root"' EXIT
mkdir -p "$root/usr/bin" "$root/DEBIAN"
install -m 0755 "$bin" "$root/usr/bin/aegis"

host_arch="$(dpkg --print-architecture 2>/dev/null || echo unknown)"
if [ "$arch" = "$host_arch" ]; then
	libexec="$root/usr/lib/aegis"; mkdir -p "$libexec"
	[ -x deploy/opencode/bin/opencode ] && install -m 0755 deploy/opencode/bin/opencode "$libexec/opencode"
	[ -x deploy/opencode/bin/rg ] && install -m 0755 deploy/opencode/bin/rg "$libexec/rg"
	[ -d deploy/opencode/oc-config ] && cp -r deploy/opencode/oc-config "$libexec/oc-config"
	[ -x deploy/llama-server/bin/llama-server ] && install -m 0755 deploy/llama-server/bin/llama-server "$libexec/llama-server"
	echo "build-deb: .deb($arch) bundles the harness into /usr/lib/aegis [$(ls "$libexec" 2>/dev/null | tr '\n' ' ')]" >&2
else
	echo "build-deb: NOTE — .deb($arch) is binary-only (helpers are host-arch=$host_arch)" >&2
fi

cat >"$root/DEBIAN/control" <<CTL
Package: aegis
Version: $VERSION
Section: utils
Priority: optional
Architecture: $arch
Maintainer: ioTACTICAL LLC <dev@rtmx.ai>
Description: aegis-cli — air-gap-native agentic coding orchestrator (bundles OpenCode + ripgrep + llama-server under /usr/lib/aegis)
CTL

deb="$out/aegis_${VERSION}_${arch}.deb"
dpkg-deb --root-owner-group --build "$root" "$deb" >/dev/null
echo "$deb"
