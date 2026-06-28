#!/usr/bin/env bash
# enclave-smoke.sh — closed-host smoke (ENCLAVE-003): INSTALL the package to a clean prefix,
# then prove the INSTALLED aegis brings the whole stack up and closes a real requirement with
# NO network beyond loopback. This validates the packaging chain (REL-005/006) end-to-end on a
# network-disabled host — the same `bin/aegis + libexec/` layout brew/apt install.
#
# Run it on a genuinely offline host, OR locally under the egress gate to simulate one:
#   scripts/verify-airgap.sh -- scripts/enclave-smoke.sh
# Requires the built stack (make ci-full) + a model GGUF (MODEL_OUT or a staged
# deploy/models/<MODEL_REF name>); gated like integration-smoke, exits with guidance if missing.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"

VERSION="$(tr -d ' \n' < VERSION 2>/dev/null || echo dev)"
host_os="$(go env GOOS 2>/dev/null || echo linux)"
host_arch="$(go env GOARCH 2>/dev/null || echo amd64)"

[ -x ./bin/aegis ] || { echo "enclave-smoke: ./bin/aegis missing — run 'make build'." >&2; exit 1; }
[ -x deploy/opencode/bin/opencode ] || { echo "enclave-smoke: OpenCode missing — run 'make ci-full'." >&2; exit 1; }

# Assemble the host bundle "package" (the brew/release tarball), reusing the release assembler.
DIST="${DIST:-dist}"; mkdir -p "$DIST"
cp ./bin/aegis "$DIST/aegis-$VERSION-$host_os-$host_arch"
tarball="$DIST/aegis-$VERSION-$host_os-$host_arch.tar.gz"
[ -f "$tarball" ] || scripts/build-bundle.sh "$host_os" "$host_arch" "$VERSION" "$DIST" >/dev/null
echo "enclave-smoke: package = $tarball"

# 1. INSTALL — extract the package to a clean prefix (bin/ + libexec/), exactly as a package would.
root="$(mktemp -d)"; PREFIX="$root/opt/aegis"; mkdir -p "$PREFIX"
trap 'rm -rf "$root"' EXIT
tar -C "$PREFIX" -xzf "$tarball"
INSTALLED="$PREFIX/bin/aegis"
export AEGIS_LIBEXEC="$PREFIX/libexec"
[ -x "$INSTALLED" ] || { echo "enclave-smoke: installed aegis not found at $INSTALLED" >&2; exit 1; }
echo "enclave-smoke: installed to $PREFIX (libexec: $(ls "$AEGIS_LIBEXEC" | tr '\n' ' '))"

# 2. The INSTALLED aegis must resolve its helpers from the install layout (REL-005) and report
#    closed + traceable — with NO source tree on PATH.
echo "enclave-smoke: installed verify-env…"
"$INSTALLED" verify-env --check-opencode 2>&1 | sed 's/^/  verify-env| /' || {
	echo "enclave-smoke: FAIL — installed aegis verify-env did not pass" >&2; exit 1; }

# 3. Drive the full stack FROM THE INSTALLED PACKAGE and close a real task. The egress gate is
#    the outer wrapper / the offline host (ENCLAVE_OUTER_GATE skips the inner re-wrap).
echo "enclave-smoke: driving the installed package through the full stack…"
ENCLAVE_OUTER_GATE=1 AEGIS_BIN="$INSTALLED" AEGIS_LIBEXEC="$AEGIS_LIBEXEC" scripts/integration-smoke.sh

echo "enclave-smoke: PASS — the installed package brought the stack up and closed a task offline"
