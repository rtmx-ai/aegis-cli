#!/usr/bin/env bash
# rtmx:req REQ-TEST-036
# Tests for the BDD scenario quality linter (scripts/bdd-lint.sh).
#
# Creates temporary .feature files (good and bad), runs the linter,
# and asserts correct exit codes and output patterns.
#
# Usage: bash scripts/test-bdd-lint.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LINTER="$SCRIPT_DIR/bdd-lint.sh"
PASS=0
FAIL=0

tmpdir=""
cleanup() {
    if [ -n "$tmpdir" ] && [ -d "$tmpdir" ]; then
        rm -rf "$tmpdir"
    fi
}
trap cleanup EXIT

tmpdir="$(mktemp -d)"

assert_exit() {
    local test_name="$1"
    local expected_exit="$2"
    local dir="$3"
    local output
    local actual_exit=0

    output="$(bash "$LINTER" "$dir" 2>&1)" || actual_exit=$?

    if [ "$actual_exit" -ne "$expected_exit" ]; then
        echo "FAIL: $test_name"
        echo "  Expected exit $expected_exit, got $actual_exit"
        echo "  Output: $output"
        FAIL=$((FAIL + 1))
        return
    fi
    PASS=$((PASS + 1))
    echo "PASS: $test_name"
}

assert_exit_and_match() {
    local test_name="$1"
    local expected_exit="$2"
    local pattern="$3"
    local dir="$4"
    local output
    local actual_exit=0

    output="$(bash "$LINTER" "$dir" 2>&1)" || actual_exit=$?

    if [ "$actual_exit" -ne "$expected_exit" ]; then
        echo "FAIL: $test_name (exit code)"
        echo "  Expected exit $expected_exit, got $actual_exit"
        echo "  Output: $output"
        FAIL=$((FAIL + 1))
        return
    fi

    if ! echo "$output" | grep -qE "$pattern"; then
        echo "FAIL: $test_name (pattern match)"
        echo "  Expected pattern: $pattern"
        echo "  Output: $output"
        FAIL=$((FAIL + 1))
        return
    fi
    PASS=$((PASS + 1))
    echo "PASS: $test_name"
}

# ---------------------------------------------------------------------------
# Test 1: Valid scenario passes
# ---------------------------------------------------------------------------
dir1="$tmpdir/t1"
mkdir -p "$dir1"
cat > "$dir1/good.feature" <<'EOF'
Feature: Good feature

  # @req REQ-TEST-001
  Scenario: A valid scenario
    Given some precondition
    When something happens
    Then something is asserted
EOF
assert_exit "valid scenario passes" 0 "$dir1"

# ---------------------------------------------------------------------------
# Test 2: Missing Given step
# ---------------------------------------------------------------------------
dir2="$tmpdir/t2"
mkdir -p "$dir2"
cat > "$dir2/bad.feature" <<'EOF'
Feature: Missing Given

  # @req REQ-TEST-002
  Scenario: No given step
    When something happens
    Then something is asserted
EOF
assert_exit_and_match "missing Given detected" 1 "MISSING_GIVEN_WHEN_THEN.*missing Given" "$dir2"

# ---------------------------------------------------------------------------
# Test 3: Missing When step
# ---------------------------------------------------------------------------
dir3="$tmpdir/t3"
mkdir -p "$dir3"
cat > "$dir3/bad.feature" <<'EOF'
Feature: Missing When

  # @req REQ-TEST-003
  Scenario: No when step
    Given some precondition
    Then something is asserted
EOF
assert_exit_and_match "missing When detected" 1 "MISSING_GIVEN_WHEN_THEN.*missing.*When" "$dir3"

# ---------------------------------------------------------------------------
# Test 4: Missing Then step
# ---------------------------------------------------------------------------
dir4="$tmpdir/t4"
mkdir -p "$dir4"
cat > "$dir4/bad.feature" <<'EOF'
Feature: Missing Then

  # @req REQ-TEST-004
  Scenario: No then step
    Given some precondition
    When something happens
EOF
assert_exit_and_match "missing Then detected" 1 "MISSING_GIVEN_WHEN_THEN.*missing.*Then" "$dir4"

# ---------------------------------------------------------------------------
# Test 5: Missing @req tag
# ---------------------------------------------------------------------------
dir5="$tmpdir/t5"
mkdir -p "$dir5"
cat > "$dir5/bad.feature" <<'EOF'
Feature: Missing req tag

  Scenario: No req marker
    Given some precondition
    When something happens
    Then something is asserted
EOF
assert_exit_and_match "missing @req tag detected" 1 "MISSING_REQ_TAG" "$dir5"

# ---------------------------------------------------------------------------
# Test 6: Implementation detail in Then (double colon)
# ---------------------------------------------------------------------------
dir6="$tmpdir/t6"
mkdir -p "$dir6"
cat > "$dir6/bad.feature" <<'EOF'
Feature: Impl detail

  # @req REQ-TEST-006
  Scenario: Uses struct path in Then
    Given some precondition
    When something happens
    Then DlpFilter::check should return true
EOF
assert_exit_and_match "impl detail :: detected" 1 "IMPL_DETAIL_IN_THEN.*::" "$dir6"

# ---------------------------------------------------------------------------
# Test 7: Implementation detail in Then (struct keyword)
# ---------------------------------------------------------------------------
dir7="$tmpdir/t7"
mkdir -p "$dir7"
cat > "$dir7/bad.feature" <<'EOF'
Feature: Impl detail struct

  # @req REQ-TEST-007
  Scenario: Uses struct in Then
    Given some precondition
    When something happens
    Then the struct AppState should have field x
EOF
assert_exit_and_match "impl detail struct detected" 1 "IMPL_DETAIL_IN_THEN.*struct" "$dir7"

# ---------------------------------------------------------------------------
# Test 8: Implementation detail in Then (.rs file extension)
# ---------------------------------------------------------------------------
dir8="$tmpdir/t8"
mkdir -p "$dir8"
cat > "$dir8/bad.feature" <<'EOF'
Feature: Impl detail rs

  # @req REQ-TEST-008
  Scenario: References .rs file in Then
    Given some precondition
    When something happens
    Then the output in main.rs should be correct
EOF
assert_exit_and_match "impl detail .rs detected" 1 "IMPL_DETAIL_IN_THEN.*\.rs" "$dir8"

# ---------------------------------------------------------------------------
# Test 9: Implementation detail in Then (camelCase)
# ---------------------------------------------------------------------------
dir9="$tmpdir/t9"
mkdir -p "$dir9"
cat > "$dir9/bad.feature" <<'EOF'
Feature: Impl detail camelCase

  # @req REQ-TEST-009
  Scenario: Uses camelCase in Then
    Given some precondition
    When something happens
    Then parseSlashCommand should return success
EOF
assert_exit_and_match "impl detail camelCase detected" 1 "IMPL_DETAIL_IN_THEN.*parseSlashCommand" "$dir9"

# ---------------------------------------------------------------------------
# Test 10: Empty scenario
# ---------------------------------------------------------------------------
dir10="$tmpdir/t10"
mkdir -p "$dir10"
cat > "$dir10/bad.feature" <<'EOF'
Feature: Empty scenario

  # @req REQ-TEST-010
  Scenario: This scenario has no steps

  # @req REQ-TEST-011
  Scenario: Another scenario with steps
    Given something
    When something
    Then something
EOF
assert_exit_and_match "empty scenario detected" 1 "EMPTY_SCENARIO" "$dir10"

# ---------------------------------------------------------------------------
# Test 11: And/But inherit step type correctly
# ---------------------------------------------------------------------------
dir11="$tmpdir/t11"
mkdir -p "$dir11"
cat > "$dir11/good.feature" <<'EOF'
Feature: And/But inheritance

  # @req REQ-TEST-011
  Scenario: Uses And after Given/When/Then
    Given precondition one
    And precondition two
    When action one
    And action two
    Then result one
    And result two
EOF
assert_exit "And/But inheritance passes" 0 "$dir11"

# ---------------------------------------------------------------------------
# Test 12: Scenario Outline is linted
# ---------------------------------------------------------------------------
dir12="$tmpdir/t12"
mkdir -p "$dir12"
cat > "$dir12/bad.feature" <<'EOF'
Feature: Outline missing req

  Scenario Outline: Parameterized without req
    Given precondition with <param>
    When action happens
    Then result with <param>

    Examples:
      | param |
      | foo   |
EOF
assert_exit_and_match "Scenario Outline missing @req detected" 1 "MISSING_REQ_TAG" "$dir12"

# ---------------------------------------------------------------------------
# Test 13: No .feature files exits 0
# ---------------------------------------------------------------------------
dir13="$tmpdir/t13"
mkdir -p "$dir13"
assert_exit "empty directory exits 0" 0 "$dir13"

# ---------------------------------------------------------------------------
# Test 14: Impl detail allowed in Given/When (only Then is checked)
# ---------------------------------------------------------------------------
dir14="$tmpdir/t14"
mkdir -p "$dir14"
cat > "$dir14/good.feature" <<'EOF'
Feature: Impl detail in Given ok

  # @req REQ-TEST-014
  Scenario: Implementation details in Given and When are fine
    Given the module crate::security is loaded
    And struct Config is initialized
    When the fn process is called
    Then the output should be successful
EOF
assert_exit "impl detail in Given/When allowed" 0 "$dir14"

# ---------------------------------------------------------------------------
# Test 15: Multiple violations in one file
# ---------------------------------------------------------------------------
dir15="$tmpdir/t15"
mkdir -p "$dir15"
cat > "$dir15/multi.feature" <<'EOF'
Feature: Multiple problems

  Scenario: Missing everything
    When something

  # @req REQ-TEST-015
  Scenario: Impl detail in Then
    Given something
    When something
    Then MyStruct::method should work
EOF
assert_exit_and_match "multiple violations detected" 1 "violation" "$dir15"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=== Test Summary ==="
echo "PASS: $PASS"
echo "FAIL: $FAIL"
TOTAL=$((PASS + FAIL))
echo "TOTAL: $TOTAL"

if [ "$FAIL" -gt 0 ]; then
    echo "Some tests failed."
    exit 1
fi
echo "All tests passed."
exit 0
