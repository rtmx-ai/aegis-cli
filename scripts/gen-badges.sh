#!/usr/bin/env bash
# gen-badges.sh — regenerate the live README badges' data each CI run.
#
# Writes shields.io "endpoint" JSON files into badges/ for the two badges whose
# value is computed from the build (coverage, version). The other three badges
# are served live by their providers and need no data file:
#   - CI status   -> GitHub Actions native badge
#   - Go grade    -> goreportcard.com
#   - License     -> shields github/license (reads LICENSE)
#
# CI publishes badges/ to the orphan `badges` branch; the README endpoint badges
# read the raw JSON from there. Runnable locally (single source of truth):
#   scripts/gen-badges.sh
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"
OUT="$REPO_ROOT/badges"
mkdir -p "$OUT"

# --- coverage (statement coverage across the whole module) --------------------
cov_pct="0.0"
if cov_out="$(mktemp)"; go test -coverpkg=./... -coverprofile="$cov_out" ./... >/dev/null 2>&1; then
	cov_pct="$(go tool cover -func="$cov_out" | awk '/^total:/ {gsub(/%/,"",$3); print $3}')"
fi
rm -f "$cov_out" 2>/dev/null || true
[ -n "$cov_pct" ] || cov_pct="0.0"

# color by threshold
cov_int="${cov_pct%.*}"
if   [ "$cov_int" -ge 80 ]; then cov_color="brightgreen"
elif [ "$cov_int" -ge 70 ]; then cov_color="green"
elif [ "$cov_int" -ge 60 ]; then cov_color="yellowgreen"
elif [ "$cov_int" -ge 45 ]; then cov_color="yellow"
else                             cov_color="orange"
fi

cat >"$OUT/coverage.json" <<EOF
{"schemaVersion":1,"label":"coverage","message":"${cov_pct}%","color":"${cov_color}"}
EOF

# --- component version --------------------------------------------------------
ver="$(tr -d ' \n' < VERSION 2>/dev/null || echo dev)"
[ -n "$ver" ] || ver="dev"
cat >"$OUT/version.json" <<EOF
{"schemaVersion":1,"label":"version","message":"v${ver}","color":"blue"}
EOF

echo "gen-badges: coverage=${cov_pct}% (${cov_color}), version=v${ver}"
echo "wrote $OUT/coverage.json $OUT/version.json"
