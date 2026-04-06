#!/usr/bin/env bash
# aegis-cli development session
#
# Two-pane layout:
#   Left:  Claude Code (interactive session with critical path prompt)
#   Right: aegis-cli TUI running in ~/aegis-sandbox (full terminal control)
#
# When aegis exits (Ctrl+C), it waits for source changes then rebuilds
# and restarts automatically.
#
# Usage:
#   ./scripts/dev.sh              # launch dev session
#   ./scripts/dev.sh attach       # reattach
#   ./scripts/dev.sh kill         # tear down

set -euo pipefail

SESSION="aegis-dev"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

CLAUDE_PROMPT='Describe the critical path. Propose a plan to accomplish the next increment of the critical path. Highlight any unmet dependencies, validate existing dependencies, and explore any requirements that need to be further decomposed.'

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
if ! command -v claude >/dev/null 2>&1; then
    echo "claude not found in PATH."
    exit 1
fi

# Ensure working tree is current
echo "Pulling latest..."
cd "$PROJECT_DIR"
git pull --ff-only || true

# Ensure sandbox clone exists and is current
SANDBOX="$HOME/aegis-sandbox"
if [ ! -d "$SANDBOX/.git" ]; then
    echo "Creating sandbox clone at $SANDBOX..."
    git clone https://github.com/rtmx-ai/aegis-cli.git "$SANDBOX"
else
    echo "Updating sandbox..."
    git -C "$SANDBOX" pull --ff-only || true
fi

tmux kill-session -t "$SESSION" 2>/dev/null || true

# Session options
tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR"
tmux set-option -t "$SESSION" -g mouse on
tmux set-option -t "$SESSION" -g status-left "#[fg=black,bg=green,bold] aegis-dev "
tmux set-option -t "$SESSION" -g status-right "#[fg=white,dim] C-b arrows:pane | C-b d:detach | C-b z:zoom "
tmux set-option -t "$SESSION" -g status-right-length 60
tmux set-option -t "$SESSION" -g status-style "bg=black,fg=white"

# Left pane: Claude Code (interactive session, prompt as positional arg)
tmux send-keys -t "$SESSION:0.0" \
  "cd $PROJECT_DIR && claude \"$CLAUDE_PROMPT\"" Enter

# Right pane: aegis-cli with auto-rebuild on source changes
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION:0.1" \
  "RUST_LOG=info $PROJECT_DIR/scripts/dev-run.sh" Enter

# Focus left pane (Claude Code)
tmux select-pane -t "$SESSION:0.0"

tmux attach-session -t "$SESSION"
