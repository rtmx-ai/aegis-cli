#!/usr/bin/env bash
# Verify VHS tape scripts exist and are well-formed.
#
# This is a syntactic, CI-friendly check that does not require `vhs` to be
# installed. It guards against accidental deletion or truncation of the
# demo tape scripts (REQ-BUILD-014, REQ-BUILD-017, REQ-BUILD-018).
#
# Usage: ./scripts/verify-tapes.sh
set -euo pipefail

TAPES_DIR="docs/demos/tapes"

[ -d "$TAPES_DIR" ] || { echo "FAIL: $TAPES_DIR missing"; exit 1; }

TAPE_COUNT=$(find "$TAPES_DIR" -name "*.tape" | wc -l | tr -d '[:space:]')
[ "$TAPE_COUNT" -ge 2 ] || {
    echo "FAIL: expected >= 2 tape files, found $TAPE_COUNT"
    exit 1
}

# Basic well-formedness: each tape declares Output and at least one action
# (Type, Sleep, or Enter). This catches stub/empty tapes.
for tape in "$TAPES_DIR"/*.tape; do
    grep -q "^Output " "$tape" || {
        echo "FAIL: $tape missing Output directive"
        exit 1
    }
    grep -qE "^(Type|Sleep|Enter)" "$tape" || {
        echo "FAIL: $tape has no actions (Type/Sleep/Enter)"
        exit 1
    }
done

# Ensure the two REQ-specific tapes exist at their required paths
# (REQ-BUILD-017 and REQ-BUILD-018 reference these exact filenames).
for required in "$TAPES_DIR/hero.tape" "$TAPES_DIR/hitl-approval.tape"; do
    [ -f "$required" ] || {
        echo "FAIL: required tape $required missing"
        exit 1
    }
done

# Ensure each required tape has its rtmx:req marker so traceability
# tooling can link tape -> requirement.
grep -q "rtmx:req REQ-BUILD-017" "$TAPES_DIR/hero.tape" || {
    echo "FAIL: $TAPES_DIR/hero.tape missing 'rtmx:req REQ-BUILD-017' marker"
    exit 1
}
grep -q "rtmx:req REQ-BUILD-018" "$TAPES_DIR/hitl-approval.tape" || {
    echo "FAIL: $TAPES_DIR/hitl-approval.tape missing 'rtmx:req REQ-BUILD-018' marker"
    exit 1
}

echo "PASS: $TAPE_COUNT tape files verified"
