#!/usr/bin/env bash
# Watch for source changes, rebuild, and restart aegis in the sandbox.
# Gives aegis full terminal control (no bacon wrapper).
#
# Uses watchexec if available, falls back to a poll loop.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_DIR/target/debug/aegis"
SANDBOX="$HOME/aegis-sandbox"

# Ensure sandbox exists
if [ ! -d "$SANDBOX/.git" ]; then
    echo "Creating sandbox clone at $SANDBOX..."
    git clone https://github.com/rtmx-ai/aegis-cli.git "$SANDBOX"
fi

# Initial build
echo "Building aegis..."
cd "$PROJECT_DIR"
cargo build --package aegis-cli 2>&1

echo "Starting aegis in $SANDBOX (Ctrl+C to stop)..."
echo "File changes in crates/ will trigger rebuild + restart."
echo ""

# Run aegis, rebuild on source changes
while true; do
    cd "$SANDBOX"
    "$BINARY" chat || true

    # If aegis exits, wait for a file change then rebuild
    echo ""
    echo "[aegis exited -- waiting for source changes to rebuild...]"
    cd "$PROJECT_DIR"

    # Wait for any .rs or .toml change in crates/
    if command -v fswatch >/dev/null 2>&1; then
        fswatch -1 -r -e "target" --include="\\.(rs|toml)$" crates/ Cargo.toml
    else
        # Fallback: poll every 2 seconds
        sleep 2
    fi

    echo "[rebuilding...]"
    cargo build --package aegis-cli 2>&1 || continue
    echo "[restarting aegis...]"
    echo ""
done
