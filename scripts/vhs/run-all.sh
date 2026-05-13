#!/usr/bin/env bash
# Run all VHS tape files and output GIFs to docs/demos/gifs/
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUTPUT_DIR="$PROJECT_DIR/docs/demos/gifs"
TAPE_DIR="$PROJECT_DIR/docs/demos/tapes"

mkdir -p "$OUTPUT_DIR"

if ! command -v vhs &>/dev/null; then
    echo "ERROR: vhs not found. Install via: go install github.com/charmbracelet/vhs@latest" >&2
    exit 1
fi

success=0
failure=0

for tape in "$TAPE_DIR"/*.tape; do
    [ -f "$tape" ] || continue
    name="$(basename "$tape" .tape)"
    echo "Recording $name..."
    if vhs "$tape" -o "$OUTPUT_DIR/${name}.gif"; then
        ((success++))
    else
        echo "FAILED: $name" >&2
        ((failure++))
    fi
done

echo "Done: $success succeeded, $failure failed"
[ "$failure" -eq 0 ] || exit 1
