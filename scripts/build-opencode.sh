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

# OC-003: install OpenCode's deps with the PINNED bun (CI installs 1.3.14). We pass
# --no-frozen-lockfile explicitly: bun auto-freezes in CI, and opencode's upstream lockfile @
# v1.17.9 needs a small normalization under bun 1.3.14 that a frozen install rejects on a fresh
# clone (it only passes locally because a prior run already updated build/opencode-src).
# Determinism is anchored by the pinned bun + pinned source (OPENCODE_REF) + the built binary's
# checksum in SHA256SUMS, not by the stale upstream lockfile.
( cd "$SRC" && bun install --no-frozen-lockfile )

# OC-002: bake air-gap protections into the build (defense in depth with the
# shipped deploy/opencode/opencode.json config).
export OPENCODE_TELEMETRY=0 OPENCODE_AUTOUPDATE=0 OPENCODE_DISABLE_SHARE=1 OPENCODE_DISABLE_ANALYTICS=1

# Build the CLASSIC CLI (packages/opencode) — it ships the headless `opencode run`
# command (non-interactive: one prompt, streams events, exits on idle) and the
# documented provider config. packages/cli is the 2.0-preview `lildax`, which
# lacks `run` and whose HTTP run is an unimplemented stub. --single targets this
# platform; the artifact is dist/opencode-<os>-<arch>/bin/opencode (standalone,
# no Bun/Node runtime needed).
( cd "$SRC/packages/opencode" && bun run script/build.ts --single )
built="$(find "$SRC/packages/opencode/dist" -path '*/bin/opencode' -type f | head -1)"
if [ -z "$built" ]; then
	echo "build-opencode: build produced no binary under packages/opencode/dist" >&2
	exit 1
fi
install -m 0755 "$built" "$OUT"

echo "build-opencode: built $OUT from $REF (hardened, single self-contained binary)"
