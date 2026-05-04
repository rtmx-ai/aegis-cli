#!/usr/bin/env bash
# rtmx:req REQ-TEST-033
# Tests for scripts/coverage-delta.sh
#
# Creates mock git repos with .rtmx/database.csv, runs coverage-delta.sh
# in non-posting mode, and verifies the output.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COVERAGE_SCRIPT="${SCRIPT_DIR}/coverage-delta.sh"

PASS=0
FAIL=0
TMPDIR_TEST="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_TEST"' EXIT

assert_contains() {
  local label="$1"
  local haystack="$2"
  local needle="$3"
  if echo "$haystack" | grep -qF "$needle"; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label -- expected to find: $needle"
    FAIL=$((FAIL + 1))
  fi
}

assert_not_contains() {
  local label="$1"
  local haystack="$2"
  local needle="$3"
  if echo "$haystack" | grep -qF "$needle"; then
    echo "  FAIL: $label -- did NOT expect to find: $needle"
    FAIL=$((FAIL + 1))
  else
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  fi
}

# --- Setup: create a mock git repo with two commits -----------------------

MOCK_REPO="${TMPDIR_TEST}/mock-repo"
mkdir -p "${MOCK_REPO}/.rtmx"
cd "$MOCK_REPO"
git init -q
git config user.email "test@test.com"
git config user.name "Test"

CSV_HEADER="req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date,requirement_file,external_id"

# Base commit: 5 COMPLETE, 5 MISSING (50%)
cat > .rtmx/database.csv <<EOF
${CSV_HEADER}
REQ-FOO-001,TEST,UNIT,First requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-002,TEST,UNIT,Second requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-003,TEST,UNIT,Third requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-004,TEST,UNIT,Fourth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-005,TEST,UNIT,Fifth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-006,TEST,UNIT,Sixth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-007,TEST,UNIT,Seventh requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-008,TEST,UNIT,Eighth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-009,TEST,UNIT,Ninth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-010,TEST,UNIT,Tenth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
EOF

git add .rtmx/database.csv
git commit -q -m "base: 5 complete, 5 missing"
BASE_REF="$(git rev-parse HEAD)"

# Head commit: 6 COMPLETE, 4 MISSING (60%) -- REQ-FOO-006 gains coverage
cat > .rtmx/database.csv <<EOF
${CSV_HEADER}
REQ-FOO-001,TEST,UNIT,First requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-002,TEST,UNIT,Second requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-003,TEST,UNIT,Third requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-004,TEST,UNIT,Fourth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-005,TEST,UNIT,Fifth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-006,TEST,UNIT,Sixth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-007,TEST,UNIT,Seventh requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-008,TEST,UNIT,Eighth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-009,TEST,UNIT,Ninth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-010,TEST,UNIT,Tenth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
EOF

git add .rtmx/database.csv
git commit -q -m "head: 6 complete, 4 missing"
HEAD_REF="$(git rev-parse HEAD)"

# --- Test 1: Delta table is present and correct ----------------------------

echo "Test 1: Delta table output"
OUTPUT="$(bash "$COVERAGE_SCRIPT" "$BASE_REF" "$HEAD_REF")"

assert_contains "header present" "$OUTPUT" "## RTMX Requirements Coverage"
assert_contains "base row" "$OUTPUT" "5 / 10"
assert_contains "base pct" "$OUTPUT" "50.0%"
assert_contains "head row" "$OUTPUT" "6 / 10"
assert_contains "head pct" "$OUTPUT" "60.0%"
assert_contains "delta reqs" "$OUTPUT" "**+1**"
assert_contains "delta pct" "$OUTPUT" "**+10.0%**"

# --- Test 2: Newly covered requirement listed -----------------------------

echo "Test 2: Newly covered requirement listed"
assert_contains "gained req" "$OUTPUT" "REQ-FOO-006"
assert_contains "gained section" "$OUTPUT" "### New requirements covered"

# --- Test 3: No lost coverage ---------------------------------------------

echo "Test 3: No lost coverage"
# The "Requirements lost coverage" section should say (none)
LOST_SECTION="$(echo "$OUTPUT" | sed -n '/### Requirements lost coverage/,$ p')"
assert_contains "lost none" "$LOST_SECTION" "(none)"

# --- Test 4: Lost coverage scenario ---------------------------------------

echo "Test 4: Lost coverage scenario"

# New commit where REQ-FOO-001 loses coverage
cat > .rtmx/database.csv <<EOF
${CSV_HEADER}
REQ-FOO-001,TEST,UNIT,First requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-002,TEST,UNIT,Second requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-003,TEST,UNIT,Third requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-004,TEST,UNIT,Fourth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-005,TEST,UNIT,Fifth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-006,TEST,UNIT,Sixth requirement,passes,mod,func,Unit Test,COMPLETE,HIGH,1,,1,,,,,,,,
REQ-FOO-007,TEST,UNIT,Seventh requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-008,TEST,UNIT,Eighth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-009,TEST,UNIT,Ninth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
REQ-FOO-010,TEST,UNIT,Tenth requirement,passes,mod,func,Unit Test,MISSING,HIGH,1,,1,,,,,,,,
EOF

git add .rtmx/database.csv
git commit -q -m "lost: REQ-FOO-001 regressed"
REGRESSED_REF="$(git rev-parse HEAD)"

OUTPUT2="$(bash "$COVERAGE_SCRIPT" "$HEAD_REF" "$REGRESSED_REF")"
assert_contains "lost req listed" "$OUTPUT2" "REQ-FOO-001"
assert_contains "lost section" "$OUTPUT2" "### Requirements lost coverage"
assert_contains "delta negative" "$OUTPUT2" "**-1**"

# --- Test 5: Zero delta ---------------------------------------------------

echo "Test 5: Zero delta"
OUTPUT3="$(bash "$COVERAGE_SCRIPT" "$HEAD_REF" "$HEAD_REF")"
assert_contains "zero delta reqs" "$OUTPUT3" "**0**"
assert_contains "zero delta pct" "$OUTPUT3" "**0.0%**"

# --- Summary ---------------------------------------------------------------

echo ""
echo "============================="
echo "Results: ${PASS} passed, ${FAIL} failed"
echo "============================="

if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi
