#!/usr/bin/env bash
# stage-model.sh — acquire + verify the pinned model GGUF (MODEL-002). Mirrors
# build-opencode.sh / build-llama.sh: pinned (deploy/models/MODEL_REF: name +
# sha256), VERIFIED (refuses on digest mismatch), staged for air-gap transfer.
# Side-load only: copy from a verified source dir; never fetch at runtime.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"
REF="deploy/models/MODEL_REF"
name="$(grep -o '"name"[^,]*' "$REF" | sed 's/.*: *"//; s/".*//')"
want="$(grep -o '"sha256"[^,]*' "$REF" | sed 's/.*: *"//; s/".*//')"
OUT="${MODEL_OUT:-deploy/models/$name}"
case "$want" in
	PENDING*) echo "stage-model: model pin is PENDING — finalize the SERVE-016 bake-off winner + its sha256 in $REF." >&2; exit 1 ;;
esac
SRC="${MODEL_SRC:?set MODEL_SRC to the directory holding $name}"
[ -f "$SRC/$name" ] || { echo "stage-model: $name not found under $SRC" >&2; exit 1; }
got="$(sha256sum "$SRC/$name" | cut -d' ' -f1)"
if [ "$got" != "$want" ]; then
	echo "stage-model: sha256 MISMATCH for $name — refusing (want $want, got $got)" >&2; exit 1
fi
install -m 0644 "$SRC/$name" "$OUT"
echo "stage-model: staged $OUT (sha256 verified)"
