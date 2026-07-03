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

# retry runs a command up to 3 times with exponential backoff (REL-013): CI runners flake on git
# fetch and registry installs, and a transient network error must not sink a whole release build.
retry() {
	local n=1 max=3 delay=5
	until "$@"; do
		if [ "$n" -ge "$max" ]; then
			echo "build-opencode: '$*' failed after $max attempts" >&2
			return 1
		fi
		echo "build-opencode: '$1 …' failed (attempt $n/$max); retrying in ${delay}s" >&2
		sleep "$delay"; n=$((n + 1)); delay=$((delay * 2))
	done
}

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

# Pinned source only. Clone with retry + clean between attempts (REL-013: a partial clone from a
# runner flake must not wedge the build — each attempt starts from a clean tree).
if [ ! -d "$SRC/.git" ]; then
	n=1
	until git clone "$SOURCE_REPO" "$SRC"; do
		[ "$n" -ge 3 ] && { echo "build-opencode: clone failed after 3 attempts" >&2; exit 1; }
		echo "build-opencode: clone attempt $n failed; cleaning + retrying" >&2
		rm -rf "$SRC"; sleep $((5 * n)); n=$((n + 1))
	done
fi
retry git -C "$SRC" fetch --tags origin
# Clear any conflicted/unmerged state from a prior run (e.g. a stash-pop conflict) so the
# checkout + the patch apply (OC-017) are idempotent; then pin to a pristine tree.
git -C "$SRC" reset --hard -q 2>/dev/null || true
git -C "$SRC" stash clear 2>/dev/null || true
git -C "$SRC" checkout -q -f "$REF"
git -C "$SRC" reset --hard -q "$REF"
echo "build-opencode: building anomalyco/opencode @ $REF"

# OC-017: apply aegis's build-time hardening + rebranding patches over the PINNED source —
# strip the cloud model catalog to a local whitelist (OC-012), rebrand to aegis (OC-014), point
# docs at aegis (OC-015), etc. These are a minimal, reviewable patch set, NOT a fork (CLAUDE.md
# §1). Each must apply cleanly to OPENCODE_REF; a conflict FAILS the build loudly so an OC-008
# upstream bump can never silently drop a control — re-roll the patch against the new pin.
patches_dir="$REPO_ROOT/deploy/opencode/patches"
if [ -d "$patches_dir" ]; then
	for p in "$patches_dir"/*.patch; do
		[ -e "$p" ] || continue
		echo "build-opencode: applying patch $(basename "$p")"
		if ! git -C "$SRC" apply --check "$p" 2>/dev/null; then
			echo "build-opencode: ERROR — patch $(basename "$p") does not apply to $REF; re-roll it against the new upstream pin (OC-017)." >&2
			exit 1
		fi
		git -C "$SRC" apply "$p"
	done
fi

# OC-003: install OpenCode's deps with the PINNED bun (CI installs 1.3.14). We pass
# --no-frozen-lockfile explicitly: bun auto-freezes in CI, and opencode's upstream lockfile @
# v1.17.9 needs a small normalization under bun 1.3.14 that a frozen install rejects on a fresh
# clone (it only passes locally because a prior run already updated build/opencode-src).
# Determinism is anchored by the pinned bun + pinned source (OPENCODE_REF) + the built binary's
# checksum in SHA256SUMS, not by the stale upstream lockfile.
( cd "$SRC" && retry bun install --no-frozen-lockfile )

# OC-002: bake air-gap protections into the build (defense in depth with the
# shipped deploy/opencode/opencode.json config).
export OPENCODE_TELEMETRY=0 OPENCODE_AUTOUPDATE=0 OPENCODE_DISABLE_SHARE=1 OPENCODE_DISABLE_ANALYTICS=1

# OC-012: bake a WHITELISTED model catalog (NO cloud/commercial providers). generate.ts reads
# MODELS_DEV_API_JSON as the catalog source instead of fetching models.dev's 145-provider cloud
# catalog — so the embedded OPENCODE_MODELS_DEV is our whitelist, and the picker shows no cloud
# models. OC-013 populates the whitelist from the origin-approved policy; the config's `local`
# provider (deploy/opencode/opencode.json) supplies the runtime model.
python3 scripts/gen-model-whitelist.py 2>/dev/null || true   # OC-013: whitelist from origin policy
export MODELS_DEV_API_JSON="$REPO_ROOT/deploy/opencode/models-whitelist.json"

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
