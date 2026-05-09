#!/usr/bin/env bash
# Record and compare criterion baselines.
# Usage:
#   ./scripts/bench-baseline.sh save       -- save current results as baseline
#   ./scripts/bench-baseline.sh compare    -- compare current against baseline
#   ./scripts/bench-baseline.sh            -- defaults to compare
set -euo pipefail

MODE="${1:-compare}"
BASELINE_NAME="main"

case "$MODE" in
    save)
        echo "==> Saving benchmark baseline as '${BASELINE_NAME}'"
        cargo bench --package aegis-bench -- --save-baseline "$BASELINE_NAME"
        echo "==> Baseline saved."
        ;;
    compare)
        echo "==> Comparing against baseline '${BASELINE_NAME}'"
        cargo bench --package aegis-bench -- --baseline "$BASELINE_NAME" 2>&1 | tee /dev/stderr | {
            improved=0
            regressed=0
            nochange=0
            while IFS= read -r line; do
                case "$line" in
                    *"Performance has improved"*)
                        improved=$((improved + 1))
                        ;;
                    *"Performance has regressed"*)
                        regressed=$((regressed + 1))
                        ;;
                    *"No change in performance"*)
                        nochange=$((nochange + 1))
                        ;;
                esac
            done
            echo ""
            echo "=== Baseline comparison summary ==="
            echo "  Improved:   $improved"
            echo "  Regressed:  $regressed"
            echo "  No change:  $nochange"
            if [ "$regressed" -gt 0 ]; then
                echo "  ** WARNING: $regressed benchmark(s) regressed **"
            fi
        }
        ;;
    *)
        echo "Usage: $0 {save|compare}" >&2
        exit 1
        ;;
esac
