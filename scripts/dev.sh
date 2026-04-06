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

tmux kill-session -t "$SESSION" 2>/dev/null || true

# Ensure ~/.cargo/bin is in PATH for tmux panes (Homebrew Rust installs
# cargo-installed binaries here, but tmux spawns non-login shells that
# may not have it in PATH).
PATHFIX="export PATH=\"\$HOME/.cargo/bin:\$PATH\";"

tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR"

# Left pane: editor / Claude Code
tmux send-keys -t "$SESSION" "$PATHFIX echo 'Editor pane. Run: claude'" Enter

# Right pane: bacon watch (auto-rebuild + auto-restart aegis on save)
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION" \
  "$PATHFIX RUST_LOG=info bacon watch" Enter

# Focus editor pane
tmux select-pane -t "$SESSION:0.0"

tmux attach-session -t "$SESSION"
