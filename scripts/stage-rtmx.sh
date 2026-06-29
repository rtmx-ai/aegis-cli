#!/usr/bin/env bash
# stage-rtmx.sh — stage the rtmx intent binary into deploy/rtmx/bin/rtmx for bundling (OC-019).
#
# rtmx is aegis's air-gap intent engine. aegis ships it in the package libexec so the bundled
# TUI's rtmx MCP (next/claim/verify/set_status) works OUT OF THE BOX — no operator install. The
# launch prepends libexec to PATH (internal/opencode hardenedPath), so the MCP command
# `["rtmx", ...]` resolves the bundled binary first.
#
# Source: $RTMX_SRC (a path to an rtmx binary), else the build host's rtmx on PATH. A pinned
# from-source rtmx build (like OpenCode/llama) is a future refinement; rtmx is first-party.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"

OUT="deploy/rtmx/bin/rtmx"; mkdir -p "$(dirname "$OUT")"
src="${RTMX_SRC:-$(command -v rtmx 2>/dev/null || true)}"
[ -n "$src" ] && [ -x "$src" ] || { echo "stage-rtmx: rtmx not found (set RTMX_SRC or install rtmx on PATH)" >&2; exit 1; }
install -m 0755 "$src" "$OUT"
echo "stage-rtmx: staged $OUT from $src ($("$OUT" --version 2>/dev/null | head -1))"
