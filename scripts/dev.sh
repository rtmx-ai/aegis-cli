#!/usr/bin/env bash
# aegis-cli development session
#
# Two-pane layout: editor left, aegis (auto-restart) right.
# File saves trigger rebuild + restart automatically.
#
# Usage:
#   ./scripts/dev.sh              # launch dev session
#   ./scripts/dev.sh attach       # reattach
#   ./scripts/dev.sh kill         # tear down
#
# Telemetry: tail -f ~/.aegis/debug.log* (or ! tail -20 ~/.aegis/debug.log* from Claude Code)

set -euo pipefail

SESSION="aegis-dev"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BACON="$HOME/.cargo/bin/bacon"

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

# Preflight: verify bacon exists
if [ ! -x "$BACON" ]; then
    echo "bacon not found at $BACON"
    echo "Install: cargo install bacon"
    exit 1
fi

tmux kill-session -t "$SESSION" 2>/dev/null || true
tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR"

# Right pane: bacon watch (auto-rebuild + auto-restart aegis on save)
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION:0.1" "RUST_LOG=info $BACON watch" Enter

# Left pane: ready for editor / Claude Code
tmux select-pane -t "$SESSION:0.0"

tmux attach-session -t "$SESSION"
