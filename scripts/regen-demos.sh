#!/usr/bin/env bash
# Regenerate all demo GIFs from VHS tape scripts.
#
# Requires `vhs` installed (https://github.com/charmbracelet/vhs):
#   macOS:  brew install vhs
#   Linux:  see the vhs README for binary/tarball installs
#
# The dev-loop tape records a REAL aegis session and is intentionally
# skipped here; regenerate it manually when scripts/dev.sh changes:
#   vhs docs/demos/tapes/dev-loop.tape
set -euo pipefail

TAPES_DIR="docs/demos/tapes"
OUTPUT_DIR="docs/demos"

command -v vhs >/dev/null || {
    echo "vhs not found; install from https://github.com/charmbracelet/vhs"
    exit 1
}

count=0
for tape in "$TAPES_DIR"/*.tape; do
    name=$(basename "$tape" .tape)
    # Skip the real-session dev-loop tape (regenerate manually)
    if [ "$name" = "dev-loop" ]; then
        echo "Skipping $name.tape (regenerate manually: vhs $tape)"
        continue
    fi
    echo "Generating $name.gif..."
    vhs "$tape"
    count=$((count + 1))
done

echo "Regenerated $count GIFs in $OUTPUT_DIR/"
