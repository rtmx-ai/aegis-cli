#!/usr/bin/env bash
# aegis-cli development session
#
# Launches a tmux session with three panes:
#   [0] bacon       -- hot-reload build/test on save
#   [1] aegis       -- run aegis interactively
#   [2] tail log    -- live tracing telemetry
#
# Usage:
#   ./scripts/dev.sh              # launch dev session
#   ./scripts/dev.sh attach       # reattach to existing session
#   ./scripts/dev.sh kill         # tear down session
#
# Prerequisites:
#   brew install tmux
#   cargo install bacon

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

# Kill existing session if any
tmux kill-session -t "$SESSION" 2>/dev/null || true

# Create session with first pane: bacon (hot reload)
tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR" -x 200 -y 50

# Pane 0: bacon
tmux send-keys -t "$SESSION" "bacon" Enter

# Split right: aegis run pane
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"

# Pane 1: ready to run aegis
tmux send-keys -t "$SESSION" \
  "echo '-- aegis run pane --'; echo 'Run: cargo run -- chat -p \"your prompt\"'" Enter

# Split pane 1 vertically: log tail
tmux split-window -v -t "$SESSION" -c "$PROJECT_DIR"

# Pane 2: tail debug log
tmux send-keys -t "$SESSION" \
  "touch ~/.aegis/debug.log && tail -f ~/.aegis/debug.log" Enter

# Layout: bacon on left (50%), aegis top-right, logs bottom-right
tmux select-layout -t "$SESSION" main-vertical

# Focus the aegis run pane
tmux select-pane -t "$SESSION:0.1"

# Attach
tmux attach-session -t "$SESSION"
