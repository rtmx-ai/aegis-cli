#!/usr/bin/env bash
# BDD Scenario Quality Linter (REQ-TEST-036)
#
# Enforces quality rules on .feature files:
#   1. MISSING_GIVEN_WHEN_THEN - Every Scenario must have Given+When+Then
#   2. IMPL_DETAIL_IN_THEN     - Then steps must not reference implementation details
#   3. MISSING_REQ_TAG         - Every Scenario must have a @req REQ-XXX-NNN marker
#   4. EMPTY_SCENARIO          - Scenario blocks must have at least one step
#
# Usage: ./scripts/bdd-lint.sh [directory]
#        Default directory: tests/features/
#
# Exit 0 if no violations, exit 1 if any found.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

FEATURES_DIR="${1:-$PROJECT_ROOT/tests/features}"

if [ ! -d "$FEATURES_DIR" ]; then
    echo "Error: directory not found: $FEATURES_DIR" >&2
    exit 2
fi

VIOLATIONS=0

# Collect all .feature files
mapfile -t FEATURE_FILES < <(find "$FEATURES_DIR" -name '*.feature' -type f | sort)

if [ ${#FEATURE_FILES[@]} -eq 0 ]; then
    echo "No .feature files found in $FEATURES_DIR"
    exit 0
fi

emit_violation() {
    echo "$1"
    VIOLATIONS=$((VIOLATIONS + 1))
}

for file in "${FEATURE_FILES[@]}"; do
    relative_file="${file#"$PROJECT_ROOT/"}"
    line_num=0
    in_scenario=false
    scenario_name=""
    scenario_start=0
    has_given=false
    has_when=false
    has_then=false
    has_steps=false
    has_req_tag=false
    current_step_type=""
    # Track lines above current scenario for @req comment
    prev_lines=()

    check_scenario() {
        if ! $in_scenario; then
            return
        fi

        # Rule 4: EMPTY_SCENARIO
        if ! $has_steps; then
            emit_violation "$relative_file:$scenario_start: EMPTY_SCENARIO: Scenario \"$scenario_name\" has no steps"
            return
        fi

        # Rule 1: MISSING_GIVEN_WHEN_THEN
        local missing=""
        if ! $has_given; then
            missing="Given"
        fi
        if ! $has_when; then
            if [ -n "$missing" ]; then missing="$missing, "; fi
            missing="${missing}When"
        fi
        if ! $has_then; then
            if [ -n "$missing" ]; then missing="$missing, "; fi
            missing="${missing}Then"
        fi
        if [ -n "$missing" ]; then
            emit_violation "$relative_file:$scenario_start: MISSING_GIVEN_WHEN_THEN: Scenario \"$scenario_name\" missing $missing step"
        fi

        # Rule 3: MISSING_REQ_TAG
        if ! $has_req_tag; then
            emit_violation "$relative_file:$scenario_start: MISSING_REQ_TAG: Scenario \"$scenario_name\" has no @req tag"
        fi
    }

    # Check if a string contains implementation details (for Then steps)
    # Uses bash pattern matching -- no subprocess
    has_impl_detail() {
        local s="$1"
        # Check :: (Rust path separator)
        [[ "$s" == *"::"* ]] && return 0
        # Check .rs (Rust file extension -- word boundary approximated)
        [[ "$s" =~ \.rs([^a-zA-Z]|$) ]] && return 0
        # Check keywords: fn, struct, impl, mod, crate::
        [[ "$s" =~ (^|[^a-zA-Z])(fn|struct|impl|mod)([^a-zA-Z]|$) ]] && return 0
        [[ "$s" == *"crate::"* ]] && return 0
        # Check camelCase/PascalCase identifiers: whole words with internal caps
        # Matches: parseSlashCommand, TokenStream, ProviderError, etc.
        # Excludes common proper nouns via allowlist below
        if [[ "$s" =~ (^|[[:space:]]|\"|\')([a-zA-Z]*[a-z][A-Z][a-zA-Z]*)(\"|\'|[[:space:]]|[,.]|$) ]]; then
            local candidate="${BASH_REMATCH[2]}"
            # Allowlist: proper nouns and well-known terms that are not code identifiers
            case "$candidate" in
                GitHub|macOS|iOS|DevOps|DevSecOps|OAuth|OpenAI|CloudTrail|CycloneDX|JavaScript|TypeScript|PgDn|PgUp|WebSocket|JSON*|NDJSON) ;;
                *) return 0 ;;
            esac
        fi
        return 1
    }

    # Extract the first impl detail match for reporting
    get_impl_match() {
        local s="$1"
        if [[ "$s" == *"::"* ]]; then echo "::"; return; fi
        if [[ "$s" =~ \.rs([^a-zA-Z]|$) ]]; then echo ".rs"; return; fi
        if [[ "$s" =~ (^|[^a-zA-Z])(fn)([^a-zA-Z]|$) ]]; then echo "fn"; return; fi
        if [[ "$s" =~ (^|[^a-zA-Z])(struct)([^a-zA-Z]|$) ]]; then echo "struct"; return; fi
        if [[ "$s" =~ (^|[^a-zA-Z])(impl)([^a-zA-Z]|$) ]]; then echo "impl"; return; fi
        if [[ "$s" =~ (^|[^a-zA-Z])(mod)([^a-zA-Z]|$) ]]; then echo "mod"; return; fi
        if [[ "$s" == *"crate::"* ]]; then echo "crate::"; return; fi
        if [[ "$s" =~ (^|[[:space:]]|\"|\')([a-zA-Z]*[a-z][A-Z][a-zA-Z]*)(\"|\'|[[:space:]]|[,.]|$) ]]; then
            local candidate="${BASH_REMATCH[2]}"
            case "$candidate" in
                GitHub|macOS|iOS|DevOps|DevSecOps|OAuth|OpenAI|CloudTrail|CycloneDX|JavaScript|TypeScript|PgDn|PgUp|WebSocket|JSON*|NDJSON) ;;
                *) echo "$candidate"; return ;;
            esac
        fi
        echo "unknown"
    }

    while IFS= read -r line; do
        line_num=$((line_num + 1))
        # Trim leading whitespace using bash parameter expansion
        trimmed="${line#"${line%%[![:space:]]*}"}"

        # Skip empty lines quickly
        if [ -z "$trimmed" ]; then
            continue
        fi

        # Detect Scenario or Scenario Outline start using bash pattern
        if [[ "$trimmed" == "Scenario:"* ]] || [[ "$trimmed" == "Scenario Outline:"* ]]; then
            # Check previous scenario first
            check_scenario

            # Extract scenario name
            if [[ "$trimmed" == "Scenario Outline:"* ]]; then
                scenario_name="${trimmed#Scenario Outline:}"
            else
                scenario_name="${trimmed#Scenario:}"
            fi
            # Trim leading whitespace from name
            scenario_name="${scenario_name#"${scenario_name%%[![:space:]]*}"}"

            scenario_start=$line_num
            in_scenario=true
            has_given=false
            has_when=false
            has_then=false
            has_steps=false
            has_req_tag=false
            current_step_type=""

            # Check preceding lines for @req tag
            for prev in "${prev_lines[@]}"; do
                if [[ "$prev" == *"@req"*REQ-* ]]; then
                    # More precise check with regex
                    if [[ "$prev" =~ @req[[:space:]]+REQ-[A-Z]+-[0-9]+ ]]; then
                        has_req_tag=true
                        break
                    fi
                fi
            done

            prev_lines=()
            continue
        fi

        # Track comment/tag lines for @req detection
        if [[ "$trimmed" == "#"* ]] || [[ "$trimmed" == "@"* ]]; then
            prev_lines+=("$trimmed")
            # Keep only last 10 lines
            if [ ${#prev_lines[@]} -gt 10 ]; then
                prev_lines=("${prev_lines[@]:1}")
            fi
            continue
        fi

        # Feature/Background/Rule keywords reset scenario context
        if [[ "$trimmed" == "Feature:"* ]] || [[ "$trimmed" == "Background:"* ]] || [[ "$trimmed" == "Rule:"* ]]; then
            check_scenario
            in_scenario=false
            prev_lines=()
            continue
        fi

        if ! $in_scenario; then
            continue
        fi

        # Detect steps using bash patterns (no subprocess)
        case "$trimmed" in
            "Given "*)
                has_steps=true
                has_given=true
                current_step_type="Given"
                ;;
            "When "*)
                has_steps=true
                has_when=true
                current_step_type="When"
                ;;
            "Then "*)
                has_steps=true
                has_then=true
                current_step_type="Then"
                ;;
            "And "*|"But "*)
                has_steps=true
                case "$current_step_type" in
                    Given) has_given=true ;;
                    When) has_when=true ;;
                    Then) has_then=true ;;
                esac
                ;;
            *)
                # Not a step line (Examples:, |, etc.)
                continue
                ;;
        esac

        # Rule 2: IMPL_DETAIL_IN_THEN - only check Then steps and And/But after Then
        if [ "$current_step_type" = "Then" ]; then
            if has_impl_detail "$trimmed"; then
                match="$(get_impl_match "$trimmed")"
                emit_violation "$relative_file:$line_num: IMPL_DETAIL_IN_THEN: Then step references \"$match\" (implementation detail)"
            fi
        fi

    done < "$file"

    # Check last scenario in file
    check_scenario
done

# ---------------------------------------------------------------------------
# Rule 5: DUPLICATE_STEP_TEXT - detect step definitions reuse opportunities
# (REQ-TEST-037)
#
# Finds step texts (Given/When/Then/And/But) that appear in multiple feature
# files. Identical steps across files indicate a reuse opportunity -- they
# should share a common step definition rather than duplicating logic.
# ---------------------------------------------------------------------------

declare -A STEP_FILES  # step text -> first file seen
declare -A STEP_DUPES  # step text -> "1" if already reported

for file in "${FEATURE_FILES[@]}"; do
    relative_file="${file#"$PROJECT_ROOT/"}"
    while IFS= read -r line; do
        trimmed="${line#"${line%%[![:space:]]*}"}"
        case "$trimmed" in
            "Given "*|"When "*|"Then "*|"And "*|"But "*)
                # Normalize: strip the keyword to get the step text
                step_text="${trimmed#* }"
                if [ -n "${STEP_FILES[$step_text]+x}" ]; then
                    prev_file="${STEP_FILES[$step_text]}"
                    if [ "$prev_file" != "$relative_file" ] && [ -z "${STEP_DUPES[$step_text]+x}" ]; then
                        # Report as info, not a violation -- reuse is a suggestion
                        STEP_DUPES["$step_text"]="1"
                    fi
                else
                    STEP_FILES["$step_text"]="$relative_file"
                fi
                ;;
        esac
    done < "$file"
done

DUPLICATE_COUNT=${#STEP_DUPES[@]}
if [ "$DUPLICATE_COUNT" -gt 0 ]; then
    echo ""
    echo "BDD step reuse: $DUPLICATE_COUNT step text(s) appear in multiple feature files."
    echo "Consider extracting shared step definitions for reuse."
fi

if [ "$VIOLATIONS" -gt 0 ]; then
    echo ""
    echo "BDD lint: $VIOLATIONS violation(s) found."
    exit 1
else
    echo "BDD lint: all scenarios pass quality checks."
    exit 0
fi
