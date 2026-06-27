#!/usr/bin/env bash
# stage-ripgrep.sh — side-load + verify the ripgrep binary OpenCode needs, into
# deploy/opencode/bin/rg (OC-009). Mirrors stage-model.sh: pinned (deploy/opencode/
# RIPGREP_REF: version + sha256), VERIFIED (refuses on digest mismatch), staged for
# air-gap transfer. Side-load only: copy `rg` from a verified source dir; never
# fetch at runtime.
#
# Why: OpenCode's grep tool resolves `rg` from PATH and otherwise DOWNLOADS ripgrep
# from github.com at bootstrap — a non-loopback egress that also wedges the run
# offline. aegis prepends this staged rg's dir to the launch PATH (internal/opencode
# airgapEnv) so OpenCode's which("rg") finds it and never reaches the network.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"
REF="deploy/opencode/RIPGREP_REF"
want="$(grep -o '"sha256"[^,]*' "$REF" | sed 's/.*: *"//; s/".*//')"
OUT="${RIPGREP_OUT:-deploy/opencode/bin/rg}"
case "$want" in
	PENDING*) echo "stage-ripgrep: pin is PENDING — extract rg from the upstream ripgrep release on the connected build host and set its sha256 in $REF." >&2; exit 1 ;;
esac
SRC="${RIPGREP_SRC:?set RIPGREP_SRC to the directory holding the rg binary}"
[ -f "$SRC/rg" ] || { echo "stage-ripgrep: rg not found under $SRC" >&2; exit 1; }
got="$(sha256sum "$SRC/rg" | cut -d' ' -f1)"
if [ "$got" != "$want" ]; then
	echo "stage-ripgrep: sha256 MISMATCH for rg — refusing (want $want, got $got)" >&2; exit 1
fi
mkdir -p "$(dirname "$OUT")"
install -m 0755 "$SRC/rg" "$OUT"
echo "stage-ripgrep: staged $OUT (sha256 verified)"
