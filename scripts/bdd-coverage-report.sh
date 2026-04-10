#!/usr/bin/env bash
# BDD Scenario Execution Coverage Report (REQ-TEST-031)
#
# Runs the cucumber test suite and parses output to report scenario
# pass/skip/fail counts and coverage percentage.
#
# Exit 0 always (informational, not gating).
#
# Usage: ./scripts/bdd-coverage-report.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "=== BDD Scenario Execution Coverage Report ==="
echo ""

# Run cucumber tests and capture output (allow failures)
OUTPUT=$(cargo test --test cucumber --package aegis-cli 2>&1 || true)

# Parse cucumber summary lines.
# Cucumber-rs output format varies but typically includes lines like:
#   X scenarios (Y passed, Z failed, W skipped)
#   or individual scenario results

PASSED=0
FAILED=0
SKIPPED=0
UNDEFINED=0

# Count individual scenario outcomes from cucumber output
PASSED=$(echo "$OUTPUT" | grep -c "^.*\.\.\. ok$" 2>/dev/null || echo 0)
FAILED=$(echo "$OUTPUT" | grep -c "^.*\.\.\. FAILED$" 2>/dev/null || echo 0)
SKIPPED=$(echo "$OUTPUT" | grep -c "^.*\.\.\. ignored$" 2>/dev/null || echo 0)

# Also look for cucumber-rs summary format
SUMMARY=$(echo "$OUTPUT" | grep -oE '[0-9]+ scenarios?' | head -1 || true)
if [ -n "$SUMMARY" ]; then
    TOTAL_FROM_SUMMARY=$(echo "$SUMMARY" | grep -oE '[0-9]+')
    P=$(echo "$OUTPUT" | grep -oE '[0-9]+ passed' | head -1 | grep -oE '[0-9]+' || echo 0)
    F=$(echo "$OUTPUT" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+' || echo 0)
    S=$(echo "$OUTPUT" | grep -oE '[0-9]+ skipped' | head -1 | grep -oE '[0-9]+' || echo 0)
    U=$(echo "$OUTPUT" | grep -oE '[0-9]+ undefined' | head -1 | grep -oE '[0-9]+' || echo 0)
    if [ "$P" -gt 0 ] || [ "$F" -gt 0 ] || [ "$S" -gt 0 ]; then
        PASSED=$P
        FAILED=$F
        SKIPPED=$S
        UNDEFINED=$U
    fi
fi

TOTAL=$((PASSED + FAILED + SKIPPED + UNDEFINED))

echo "Results:"
echo "  Passed:    $PASSED"
echo "  Failed:    $FAILED"
echo "  Skipped:   $SKIPPED"
echo "  Undefined: $UNDEFINED"
echo "  Total:     $TOTAL"
echo ""

if [ "$TOTAL" -gt 0 ]; then
    # Coverage = passed / total * 100
    COVERAGE=$(( (PASSED * 100) / TOTAL ))
    echo "BDD Coverage: ${COVERAGE}% (${PASSED}/${TOTAL} scenarios passing)"
else
    echo "BDD Coverage: N/A (no scenarios detected)"
fi

echo ""
echo "=== End of BDD Coverage Report ==="

# Count feature files and scenarios for context
FEATURE_COUNT=$(find tests/features -name "*.feature" 2>/dev/null | wc -l | tr -d ' ')
echo ""
echo "Feature files found: $FEATURE_COUNT"

exit 0
