#!/usr/bin/env bash
# aegis-cli development session
#
# Two-pane layout:
#   Left:  AI coding agent (claude, gemini-cli, codex, aegis, or custom)
#   Right: aegis-cli TUI running in ~/aegis-sandbox (full terminal control)
#
# The left-pane agent is modular. Set AEGIS_DEV_AGENT or pass --agent:
#   ./scripts/dev.sh                         # default: claude
#   ./scripts/dev.sh --agent gemini-cli      # use Gemini CLI
#   AEGIS_DEV_AGENT=codex ./scripts/dev.sh   # use OpenAI Codex CLI
#
# Usage:
#   ./scripts/dev.sh [--agent <name>]   # launch new session with default prompt
#   ./scripts/dev.sh --resume           # resume latest Claude Code session
#   ./scripts/dev.sh attach             # reattach to tmux
#   ./scripts/dev.sh kill               # tear down

set -euo pipefail

SESSION="aegis-dev"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

CLAUDE_PROMPT='Describe the critical path. Propose a plan to accomplish the next increment of the critical path. Highlight any unmet dependencies, validate existing dependencies, and explore any requirements that need to be further decomposed.'

# Parse args
AGENT="${AEGIS_DEV_AGENT:-claude}"
RESUME=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        attach)
            tmux attach-session -t "$SESSION"
            exit 0
            ;;
        kill)
            tmux kill-session -t "$SESSION" 2>/dev/null || true
            echo "Session '$SESSION' killed."
            exit 0
            ;;
        --agent)
            AGENT="$2"
            shift 2
            ;;
        --resume)
            RESUME=true
            shift
            ;;
        *)
            echo "Usage: dev.sh [--agent <name>] [--resume] | attach | kill"
            exit 1
            ;;
    esac
done

# Preflight: verify agent binary exists
if ! command -v "$AGENT" >/dev/null 2>&1; then
    echo "$AGENT not found in PATH."
    echo "Supported agents: claude, gemini-cli, codex, aegis, or any executable."
    echo "Set AEGIS_DEV_AGENT=<name> or pass --agent <name>."
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
tmux set-option -t "$SESSION" -g status-left "#[fg=black,bg=green,bold] aegis-dev #[fg=white,bg=black] $AGENT "
tmux set-option -t "$SESSION" -g status-right "#[fg=white,dim] C-b arrows:pane | C-b d:detach | C-b z:zoom "
tmux set-option -t "$SESSION" -g status-right-length 60
tmux set-option -t "$SESSION" -g status-style "bg=black,fg=white"

# Left pane: AI coding agent
if [ "$RESUME" = true ]; then
    # Resume the latest Claude Code session (--continue flag)
    tmux send-keys -t "$SESSION:0.0" \
      "cd $PROJECT_DIR && $AGENT --continue" Enter
else
    # New session with the default critical path prompt
    tmux send-keys -t "$SESSION:0.0" \
      "cd $PROJECT_DIR && $AGENT \"$CLAUDE_PROMPT\"" Enter
fi

# Right pane: aegis-cli with auto-rebuild on source changes
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION:0.1" \
  "RUST_LOG=info $PROJECT_DIR/scripts/dev-run.sh" Enter

# Focus left pane
tmux select-pane -t "$SESSION:0.0"

tmux attach-session -t "$SESSION"
