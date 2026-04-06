#!/usr/bin/env bash
# aegis-cli development session
#
# Two-pane layout:
#   Left:  Claude Code (editing aegis-cli source)
#   Right: aegis-cli running in ~/aegis-sandbox (dogfooding)
#
# File saves in left pane -> bacon rebuilds -> aegis restarts in sandbox.
#
# Usage:
#   ./scripts/dev.sh              # launch dev session
#   ./scripts/dev.sh attach       # reattach
#   ./scripts/dev.sh kill         # tear down

set -euo pipefail

SESSION="aegis-dev"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BACON="$HOME/.cargo/bin/bacon"
SANDBOX="$HOME/aegis-sandbox"

case "${1:-}" in
  attach)
    tmux attach-session -t "$SESSION"
    exit 0
    ;;
  kill)
    tmux kill-session -t "$SESSION" 2>/dev/null || true
    echo "Session '$SESSION' killed."
    exit 0
    ;;
esac

# Preflight checks
if [ ! -x "$BACON" ]; then
    echo "bacon not found. Install: cargo install bacon"
    exit 1
fi
if ! command -v claude >/dev/null 2>&1; then
    echo "claude not found in PATH."
    exit 1
fi

# Ensure sandbox clone exists
if [ ! -d "$SANDBOX/.git" ]; then
    echo "Creating sandbox clone at $SANDBOX..."
    git clone https://github.com/rtmx-ai/aegis-cli.git "$SANDBOX"
fi

tmux kill-session -t "$SESSION" 2>/dev/null || true
tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR"

# Left pane: Claude Code (editing source)
tmux send-keys -t "$SESSION:0.0" "claude" Enter

# Right pane: bacon watch (builds source, runs aegis in sandbox)
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION:0.1" "RUST_LOG=info $BACON watch" Enter

# Focus left pane (Claude Code)
tmux select-pane -t "$SESSION:0.0"

tmux attach-session -t "$SESSION"
