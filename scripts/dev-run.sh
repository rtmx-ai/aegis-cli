#!/usr/bin/env bash
# Wrapper for bacon watch job: build aegis, then run it in the sandbox clone.
# bacon calls this script; bacon handles kill + restart on file changes.

set -euo pipefail

SANDBOX="$HOME/aegis-sandbox"
BINARY="$(dirname "$0")/../target/debug/aegis"

# Build first
cargo build --package aegis-cli 2>&1

# Ensure sandbox clone exists
if [ ! -d "$SANDBOX/.git" ]; then
    echo "Creating sandbox clone at $SANDBOX..."
    git clone https://github.com/rtmx-ai/aegis-cli.git "$SANDBOX"
fi

# Run aegis in the sandbox directory
cd "$SANDBOX"
exec "$BINARY" chat
