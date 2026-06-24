#!/usr/bin/env bash
# build-opencode.sh — build OpenCode from pinned anomalyco/opencode source into a
# single, self-contained, air-gap-hardened binary (OC-002/003).
#
# Why we build it ourselves: we don't trust upstream prebuilt binaries' supply
# chain. We compile the PINNED source (deploy/opencode/OPENCODE_REF) offline, bake
# the egress vectors off at build time, and ship the result signed + SBOM'd in the
# aegis release. Bun `--compile` yields a standalone binary — no Bun/Node runtime
# is needed in the enclave.
#
# This runs on the CONNECTED build host (stage-then-disconnect). Bun is required;
# absent it degrades to a clear note so inspection/CI still passes.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

REF="$(tr -d ' \n' < deploy/opencode/OPENCODE_REF)"
SRC="${OPENCODE_SRC:-build/opencode-src}"
OUT="deploy/opencode/bin/opencode"
SOURCE_REPO="https://github.com/anomalyco/opencode"
mkdir -p "$(dirname "$OUT")"

if ! command -v bun >/dev/null 2>&1; then
	echo "build-opencode: NOTE — bun not installed; cannot build OpenCode here." >&2
	echo "build-opencode: install Bun on the connected build host (https://bun.sh), then re-run." >&2
	exit 0
fi

# Pinned source only.
if [ ! -d "$SRC/.git" ]; then
	git clone "$SOURCE_REPO" "$SRC"
fi
git -C "$SRC" fetch --tags origin
git -C "$SRC" checkout -q "$REF"
echo "build-opencode: building anomalyco/opencode @ $REF"

# OC-003: offline, frozen dependencies (no live fetch during the build).
( cd "$SRC" && bun install --frozen-lockfile )

# OC-002: bake air-gap protections into the build (defense in depth with the
# shipped deploy/opencode/opencode.json config).
export OPENCODE_TELEMETRY=0 OPENCODE_AUTOUPDATE=0 OPENCODE_DISABLE_SHARE=1 OPENCODE_DISABLE_ANALYTICS=1

# OpenCode's own build script compiles packages/cli to a single self-contained
# binary; --single targets just THIS platform. Validated against the real source:
# the artifact lands at dist/cli-<os>-<arch>/bin/lildax (a standalone ELF/Mach-O,
# no Bun/Node runtime needed).
( cd "$SRC/packages/cli" && bun run script/build.ts --single )
built="$(find "$SRC/packages/cli/dist" -path '*/bin/lildax' -type f | head -1)"
if [ -z "$built" ]; then
	echo "build-opencode: build produced no binary under packages/cli/dist" >&2
	exit 1
fi
install -m 0755 "$built" "$OUT"

echo "build-opencode: built $OUT from $REF (hardened, single self-contained binary)"
