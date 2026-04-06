#!/usr/bin/env bash
# aegis-cli development session
#
# Two-pane layout:
#   Left:  Claude Code --resume (editing aegis-cli source)
#   Right: aegis-cli running in ~/aegis-sandbox (dogfooding)
#
# Usage:
#   ./scripts/dev.sh              # launch dev session
#   ./scripts/dev.sh attach       # reattach
#   ./scripts/dev.sh kill         # tear down

set -euo pipefail

SESSION="aegis-dev"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BACON="$HOME/.cargo/bin/bacon"

DEFAULT_PROMPT="Describe the critical path. Propose a plan to accomplish the next increment of the critical path. Highlight any unmet dependencies, validate existing dependencies, and explore any requirements that need to be further decomposed."

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

tmux kill-session -t "$SESSION" 2>/dev/null || true
tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR"

# Left pane: Claude Code --resume (if no session selected, uses default prompt)
tmux send-keys -t "$SESSION:0.0" \
  "claude --resume -p '$DEFAULT_PROMPT'" Enter

# Right pane: bacon watch (builds source, runs aegis in sandbox)
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION:0.1" "RUST_LOG=info $BACON watch" Enter

# Focus left pane (Claude Code)
tmux select-pane -t "$SESSION:0.0"

tmux attach-session -t "$SESSION"
