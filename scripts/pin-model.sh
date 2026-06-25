#!/usr/bin/env bash
# pin-model.sh — pin a model GGUF by name + sha256 into deploy/models/MODEL_REF
# (MODEL-001). Pin-as-OUTPUT: point it at a GGUF and it records the verified pin,
# so stage-model.sh + the air-gap reproducibility gate have a concrete digest —
# no hand-editing JSON.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

gguf="${1:?usage: pin-model.sh <path-to-model.gguf>}"
[ -f "$gguf" ] || {
	echo "pin-model: not a file: $gguf" >&2
	exit 1
}
hash() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

name="$(basename "$gguf")"
echo "pin-model: hashing $name (this can take a moment for a large GGUF)…" >&2
sha="$(hash "$gguf")"
mkdir -p deploy/models
cat >deploy/models/MODEL_REF <<JSON
{
  "name": "$name",
  "sha256": "$sha",
  "note": "Pinned model GGUF. Side-load only — never fetched at runtime. Written by scripts/pin-model.sh."
}
JSON
echo "pin-model: pinned $name (sha256 $sha) -> deploy/models/MODEL_REF" >&2
