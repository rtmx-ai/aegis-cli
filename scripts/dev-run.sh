#!/usr/bin/env bash
# Watch for source changes, rebuild, and restart aegis in the sandbox.
# Gives aegis full terminal control (no bacon wrapper).
#
# A companion watcher runs in the background and polls for source file
# changes every 2 seconds. When a change is detected, it kills the
# foreground aegis process (by PID), causing the main loop to rebuild
# and restart automatically.

set -euo pipefail
set -m  # Enable job control so `fg` works and backgrounded jobs
        # get their own process groups.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_DIR/target/debug/aegis"
SANDBOX="$HOME/aegis-sandbox"
SENTINEL="$PROJECT_DIR/.aegis-rebuild"
PIDFILE="$PROJECT_DIR/.aegis-dev.pid"

# Enable tmux extended keys so Shift+Enter is distinguishable from Enter.
# Requires tmux 3.3a+. Silently ignored outside tmux.
if [ -n "${TMUX:-}" ]; then
    tmux set -g extended-keys on 2>/dev/null || true
fi

# Ensure sandbox exists
if [ ! -d "$SANDBOX/.git" ]; then
    echo "Creating sandbox clone at $SANDBOX..."
    git clone https://github.com/rtmx-ai/aegis-cli.git "$SANDBOX"
fi

# Clean up on exit.
cleanup() {
    rm -f "$SENTINEL" "$PIDFILE"
    if [ -n "${WATCH_PID:-}" ]; then
        kill "$WATCH_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Initial build
echo "Building aegis..."
cd "$PROJECT_DIR"
cargo build --package aegis-cli 2>&1

echo "Starting aegis in $SANDBOX (Ctrl+C to stop)..."
echo "File changes in crates/ will trigger rebuild + restart."
echo ""

rm -f "$SENTINEL" "$PIDFILE"
CRASH_COUNT=0

# Main loop: run aegis in the foreground, rebuild on source changes.
while true; do
    LAUNCH_TIME=$(date +%s)
    # Start companion watcher in the background. It polls for source
    # changes and kills aegis (by reading PIDFILE) when detected.
    WATCH_PID=""
    "$SCRIPT_DIR/dev-watch.sh" "$PIDFILE" "$SENTINEL" &
    WATCH_PID=$!

    # Run aegis in the foreground so it owns the terminal from the start.
    # A subshell writes its own PID (which becomes aegis's PID after exec)
    # to PIDFILE, avoiding the bg/fg race that triggers SIGTTOU when aegis
    # calls tcsetattr() while still in a background process group.
    cd "$SANDBOX"
    AEGIS_EXIT=0
    ( echo $$ > "$PIDFILE"; exec "$BINARY" chat ) || AEGIS_EXIT=$?
    rm -f "$PIDFILE"

    # Restore terminal state in case aegis was killed before its cleanup
    # ran (SIGTERM during startup, panic, etc.). Without this, raw mode,
    # mouse capture, and alternate screen leak into the next iteration.
    stty sane 2>/dev/null || true
    printf '\e[?1000l\e[?1006l\e[?1049l\e[?2004l' 2>/dev/null || true

    # Stop the watcher.
    if [ -n "${WATCH_PID:-}" ]; then
        kill "$WATCH_PID" 2>/dev/null || true
        wait "$WATCH_PID" 2>/dev/null || true
        WATCH_PID=""
    fi

    # Did the watcher trigger a rebuild, or did the user quit?
    if [ -f "$SENTINEL" ]; then
        rm -f "$SENTINEL"
    elif [ "$AEGIS_EXIT" -ne 0 ]; then
        # Crashed or killed (e.g., tmux detach disrupted terminal).
        EXIT_TIME=$(date +%s)
        ELAPSED=$(( EXIT_TIME - LAUNCH_TIME ))
        if [ "$ELAPSED" -lt 5 ]; then
            CRASH_COUNT=$(( CRASH_COUNT + 1 ))
        else
            CRASH_COUNT=1
        fi
        if [ "$CRASH_COUNT" -ge 3 ]; then
            echo "[aegis crashed $CRASH_COUNT times in a row -- waiting for source changes...]"
            CRASH_COUNT=0
            cd "$PROJECT_DIR"
            "$SCRIPT_DIR/dev-watch.sh" "$PIDFILE" "$SENTINEL" --once &
            WATCH_PID=$!
            wait "$WATCH_PID" 2>/dev/null || true
            WATCH_PID=""
            rm -f "$SENTINEL"
        else
            echo "[aegis exited unexpectedly (exit $AEGIS_EXIT) -- retrying in 2s...]"
            sleep 2
        fi
    else
        CRASH_COUNT=0
        # User quit cleanly -- wait for a file change before rebuilding.
        echo "[aegis exited -- waiting for source changes to rebuild...]"
        cd "$PROJECT_DIR"

        "$SCRIPT_DIR/dev-watch.sh" "$PIDFILE" "$SENTINEL" --once &
        WATCH_PID=$!
        wait "$WATCH_PID" 2>/dev/null || true
        WATCH_PID=""
        rm -f "$SENTINEL"
    fi

    cd "$PROJECT_DIR"

    # Build quietly -- only surface errors. On success, clear screen
    # so the TUI restarts clean without interleaved build output.
    BUILD_LOG=$(mktemp)
    if cargo build --package aegis-cli >"$BUILD_LOG" 2>&1; then
        rm -f "$BUILD_LOG"
        clear
    else
        echo "[build failed]"
        cat "$BUILD_LOG"
        rm -f "$BUILD_LOG"
        echo ""
        echo "[waiting for source changes to retry...]"
        "$SCRIPT_DIR/dev-watch.sh" "$PIDFILE" "$SENTINEL" --once &
        WATCH_PID=$!
        wait "$WATCH_PID" 2>/dev/null || true
        WATCH_PID=""
        rm -f "$SENTINEL"
        continue
    fi
done
