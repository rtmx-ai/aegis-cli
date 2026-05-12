#!/usr/bin/env bash
# generate-system-prompt.sh -- Assemble system_prompt.md from tier templates + RTM data
#
# Usage: scripts/generate-system-prompt.sh [output-path]
#   Default output: crates/aegis-agent/src/system_prompt.md
#
# Reads COMPLETE requirements from .rtmx/database.csv, fills T2/T3 template
# slots, and assembles the final system prompt with tier markers.
#
# This script is deterministic: same RTM state in, same prompt out.
# rtmx:req REQ-BUILD-076

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TEMPLATE_DIR="$ROOT_DIR/templates/system_prompt"
DATABASE="$ROOT_DIR/.rtmx/database.csv"
OUTPUT="${1:-$ROOT_DIR/crates/aegis-agent/src/system_prompt.md}"

# Verify inputs exist
for f in "$TEMPLATE_DIR/t0_identity.md" "$TEMPLATE_DIR/t1_capabilities.md" \
         "$TEMPLATE_DIR/t2_categories.md.tmpl" "$TEMPLATE_DIR/t3_requirements.md.tmpl" \
         "$DATABASE"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: required file not found: $f" >&2
        exit 1
    fi
done

# ---------------------------------------------------------------------------
# Parse RTM database for COMPLETE requirements
# ---------------------------------------------------------------------------

# Category counts: category -> complete/total
declare -A CAT_COMPLETE
declare -A CAT_TOTAL

# Requirement lines for T3
REQ_LINES=""

while IFS=',' read -r req_id category _ req_text _ _ _ _ status _; do
    # Skip header
    [ "$req_id" = "req_id" ] && continue

    # Count totals per category
    CAT_TOTAL[$category]=$(( ${CAT_TOTAL[$category]:-0} + 1 ))

    if [ "$status" = "COMPLETE" ]; then
        CAT_COMPLETE[$category]=$(( ${CAT_COMPLETE[$category]:-0} + 1 ))
        # Accumulate T3 requirement lines -- strip internal commas from req_text
        # by using the pre-parsed field
        REQ_LINES+="- **${req_id}**: ${req_text} [${category}]
"
    fi
done < "$DATABASE"

# ---------------------------------------------------------------------------
# Build T2: category summaries with counts
# ---------------------------------------------------------------------------

# Ordered category list matching template
CATEGORIES=(AGENT AUDIT BUILD CLI HITL INFRA LLM ONBOARD RTMX SECURITY TEST TUI)

# Extract category description blocks from the template
T2_CONTENT="## Capability categories

The following categories describe the functional areas of aegis. Each category
lists the number of requirements delivered out of the total tracked.

"

for cat in "${CATEGORIES[@]}"; do
    complete=${CAT_COMPLETE[$cat]:-0}
    total=${CAT_TOTAL[$cat]:-0}
    T2_CONTENT+="### ${cat} (${complete}/${total} delivered)

"
    # Extract description from template: lines between "### $cat" and next "### " or end
    desc=$(awk "/^### ${cat}\$/{found=1; next} /^### /{if(found) exit} found{print}" \
        "$TEMPLATE_DIR/t2_categories.md.tmpl")
    T2_CONTENT+="${desc}

"
done

# ---------------------------------------------------------------------------
# Build T3: requirement list
# ---------------------------------------------------------------------------

T3_CONTENT="## Delivered requirements

The following requirements have reached COMPLETE status and are verified by
passing tests in the CI pipeline. Each requirement is linked to one or more
test functions via \`// rtmx:req\` markers in the source code.

${REQ_LINES}"

# ---------------------------------------------------------------------------
# Assemble final prompt with tier markers
# ---------------------------------------------------------------------------

{
    echo "<!-- TIER:0 -->"
    cat "$TEMPLATE_DIR/t0_identity.md"
    echo ""
    echo "<!-- TIER:1 -->"
    cat "$TEMPLATE_DIR/t1_capabilities.md"
    echo ""
    echo "<!-- TIER:2 -->"
    echo "$T2_CONTENT"
    echo "<!-- TIER:3 -->"
    echo "$T3_CONTENT"
    echo "<!-- TIER:END -->"
} > "$OUTPUT"

# Report
TOTAL_REQS=0
COMPLETE_REQS=0
for cat in "${CATEGORIES[@]}"; do
    TOTAL_REQS=$(( TOTAL_REQS + ${CAT_TOTAL[$cat]:-0} ))
    COMPLETE_REQS=$(( COMPLETE_REQS + ${CAT_COMPLETE[$cat]:-0} ))
done

WORD_COUNT=$(wc -w < "$OUTPUT" | tr -d ' ')
echo "Generated $OUTPUT"
echo "  Requirements: ${COMPLETE_REQS}/${TOTAL_REQS} complete"
echo "  Word count: ${WORD_COUNT} (~$(( WORD_COUNT * 13 / 10 )) tokens estimated)"
