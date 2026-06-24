#!/usr/bin/env bash
# check-opencode-latest.sh — report the latest STABLE upstream OpenCode release
# (GitHub prerelease=false) vs our pinned ref, so the pin is bumped DELIBERATELY
# (update OPENCODE_REF -> rebuild -> re-validate OC-006), never floated. This is a
# connected-host maintenance helper; it is not part of the air-gapped runtime.
set -eu
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
pinned="$(tr -d ' \n' < "$REPO_ROOT/deploy/opencode/OPENCODE_REF")"
api="https://api.github.com/repos/anomalyco/opencode/releases?per_page=30"
latest="$(curl -fsSL "$api" 2>/dev/null | python3 -c \
  "import sys,json; rs=json.load(sys.stdin); print(next((r['tag_name'] for r in rs if not r['prerelease'] and not r['draft']), 'unknown'))" 2>/dev/null || echo unknown)"
echo "opencode pin (deploy/opencode/OPENCODE_REF): $pinned"
echo "upstream latest stable (prerelease=false):   $latest"
if [ "$latest" = "unknown" ]; then
	echo "status: could not reach GitHub (offline?) — pin unchanged"
elif [ "$pinned" = "$latest" ]; then
	echo "status: UP TO DATE"
else
	echo "status: BUMP AVAILABLE ($pinned -> $latest) — update OPENCODE_REF, rebuild, re-validate OC-006"
fi
