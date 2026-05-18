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

# Only process numbered tapes (01-hero, 02-hitl-approval, etc.) to avoid
# duplicates with the legacy named tapes (hero, hitl-approval, etc.).
# Skip dev-loop.tape which requires a real TTY and API tokens.
SKIP_PATTERN="^(hero|hitl-approval|airgapped|audit-ledger|plugin-provision|aegisignore|dev-loop)$"

success=0
failure=0
skipped=0

for tape in "$TAPE_DIR"/*.tape; do
    [ -f "$tape" ] || continue
    name="$(basename "$tape" .tape)"

    if [[ "$name" =~ $SKIP_PATTERN ]]; then
        echo "Skipping $name (legacy or manual-only tape)"
        ((skipped++))
        continue
    fi

    echo "Recording $name..."
    if vhs "$tape" -o "$OUTPUT_DIR/${name}.gif"; then
        ((success++))
    else
        echo "WARN: $name failed (expected in CI without TTY)" >&2
        ((failure++))
    fi
done

echo "Done: $success succeeded, $failure failed, $skipped skipped"
# Do not fail the job on tape rendering errors -- VHS requires a TTY
# that may not be available in all CI environments.
exit 0
