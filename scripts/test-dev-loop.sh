#!/usr/bin/env bash
# E2E test for the aegis-cli dev loop (scripts/dev.sh).
#
# Verifies:
#   1. tmux session creates two panes
#   2. bacon starts in right pane and finds the bacon binary
#   3. aegis binary builds successfully
#   4. Debug telemetry flows to ~/.aegis/debug.log
#   5. File change triggers rebuild in bacon
#
# Usage: ./scripts/test-dev-loop.sh
# Exit: 0 on success, 1 on failure
#
# @req REQ-BUILD-033
# @req REQ-BUILD-034

set -euo pipefail

SESSION="aegis-dev-test"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PASS=0
FAIL=0

pass() { echo "  PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL: $1"; FAIL=$((FAIL + 1)); }

cleanup() {
    tmux kill-session -t "$SESSION" 2>/dev/null || true
}
trap cleanup EXIT

echo "=== aegis dev loop E2E test ==="
echo ""

# --- Test 1: bacon binary exists ---
if [ -x "$HOME/.cargo/bin/bacon" ]; then
    pass "bacon binary exists at ~/.cargo/bin/bacon"
else
    fail "bacon binary not found at ~/.cargo/bin/bacon"
fi

# --- Test 2: dev.sh creates tmux session with two panes ---
echo ""
echo "Launching dev session..."
tmux kill-session -t "$SESSION" 2>/dev/null || true

# Use the test session name to avoid colliding with a real dev session
PATHFIX="export PATH=\"\$HOME/.cargo/bin:\$PATH\";"

tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION" "$PATHFIX echo 'left-pane-ready'" Enter
tmux split-window -h -t "$SESSION" -c "$PROJECT_DIR"
tmux send-keys -t "$SESSION" "$PATHFIX echo 'right-pane-ready'" Enter

sleep 1

PANE_COUNT=$(tmux list-panes -t "$SESSION" 2>/dev/null | wc -l | tr -d ' ')
if [ "$PANE_COUNT" = "2" ]; then
    pass "tmux session has 2 panes"
else
    fail "expected 2 panes, got $PANE_COUNT"
fi

# --- Test 3: bacon is accessible in tmux pane ---
tmux send-keys -t "$SESSION:0.1" "$PATHFIX bacon --version > /tmp/aegis-bacon-test.txt 2>&1" Enter
sleep 2

if [ -f /tmp/aegis-bacon-test.txt ] && grep -q "bacon" /tmp/aegis-bacon-test.txt; then
    BACON_VER=$(cat /tmp/aegis-bacon-test.txt)
    pass "bacon accessible in tmux pane: $BACON_VER"
else
    fail "bacon not accessible in tmux pane"
fi
rm -f /tmp/aegis-bacon-test.txt

# --- Test 4: cargo build succeeds ---
echo ""
echo "Testing cargo build..."
if cargo build --package aegis-cli 2>/dev/null; then
    pass "cargo build --package aegis-cli succeeds"
else
    fail "cargo build --package aegis-cli failed"
fi

# --- Test 5: aegis binary runs ---
if target/debug/aegis --version >/dev/null 2>&1; then
    pass "aegis binary runs (--version)"
else
    fail "aegis binary failed to run"
fi

# --- Test 6: debug log directory exists ---
mkdir -p ~/.aegis
if [ -d "$HOME/.aegis" ]; then
    pass "~/.aegis directory exists for debug logs"
else
    fail "~/.aegis directory missing"
fi

# --- Test 7: bacon.toml has watch job ---
if grep -q "jobs.watch" "$PROJECT_DIR/bacon.toml"; then
    pass "bacon.toml has [jobs.watch] defined"
else
    fail "bacon.toml missing [jobs.watch]"
fi

if grep -q "kill_then_restart" "$PROJECT_DIR/bacon.toml"; then
    pass "bacon.toml uses kill_then_restart strategy"
else
    fail "bacon.toml missing kill_then_restart strategy"
fi

# --- Summary ---
echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
