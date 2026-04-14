#!/usr/bin/env bash
# Companion watcher for dev-run.sh.
# Polls source files for changes and kills the aegis process when detected.
#
# Usage: dev-watch.sh <pid-file> <sentinel-file> [--once]
#   pid-file:      file containing aegis PID (read when change detected)
#   sentinel-file: touched before killing so parent knows why aegis died
#   --once:        exit after first change without killing (wait mode)

set -euo pipefail

PIDFILE="$1"
SENTINEL="$2"
ONCE="${3:-}"

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

source_checksum() {
    find "$PROJECT_DIR/crates" "$PROJECT_DIR/Cargo.toml" \
        -name '*.rs' -o -name '*.toml' 2>/dev/null \
        | sort | xargs stat -f '%m' 2>/dev/null | md5 || echo "none"
}

BEFORE=$(source_checksum)

while true; do
    sleep 2
    AFTER=$(source_checksum)
    if [ "$BEFORE" != "$AFTER" ]; then
        touch "$SENTINEL"
        if [ "$ONCE" = "--once" ]; then
            exit 0
        fi
        # Kill the aegis process directly.
        if [ -f "$PIDFILE" ]; then
            TARGET_PID=$(cat "$PIDFILE" 2>/dev/null || echo "")
            if [ -n "$TARGET_PID" ] && kill -0 "$TARGET_PID" 2>/dev/null; then
                kill -TERM "$TARGET_PID" 2>/dev/null || true
            fi
        fi
        exit 0
    fi
done
