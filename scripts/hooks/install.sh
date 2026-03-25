#!/usr/bin/env bash
# Install aegis-cli git hooks from the repo into .git/hooks/
# Run this once after cloning: ./scripts/hooks/install.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOKS_DIR="$(git rev-parse --git-dir)/hooks"

for hook in pre-commit pre-push; do
    if [ -f "$HOOKS_DIR/$hook" ] && [ ! -L "$HOOKS_DIR/$hook" ]; then
        echo "Backing up existing $hook to $hook.bak"
        mv "$HOOKS_DIR/$hook" "$HOOKS_DIR/$hook.bak"
    fi
    ln -sf "$SCRIPT_DIR/$hook" "$HOOKS_DIR/$hook"
    echo "Installed $hook hook"
done

echo "Git hooks installed. They will run automatically on commit and push."
