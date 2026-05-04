#!/usr/bin/env bash
# rtmx:req REQ-TEST-033
# Compute RTMX requirements coverage delta between two git refs and
# optionally post the result as a PR comment.
#
# Usage:
#   coverage-delta.sh [--post-comment] [base_ref] [head_ref]
#
# If base_ref / head_ref are omitted, falls back to GITHUB_BASE_REF / GITHUB_SHA.

set -euo pipefail

POST_COMMENT=false
if [[ "${1:-}" == "--post-comment" ]]; then
  POST_COMMENT=true
  shift
fi

BASE_REF="${1:-${GITHUB_BASE_REF:-main}}"
HEAD_REF="${2:-${GITHUB_SHA:-HEAD}}"

REPO_ROOT="$(git rev-parse --show-toplevel)"
DATABASE_REL=".rtmx/database.csv"

# --- helpers ---------------------------------------------------------------

# Count COMPLETE and total requirements from a database.csv file.
# Outputs two numbers: complete total
count_coverage() {
  local csv="$1"
  local complete=0
  local total=0
  # Skip header line. Use status field (column 9) but handle quoted CSV
  # fields by reading the whole line and extracting status robustly.
  while IFS= read -r line; do
    # Skip header
    [[ "$line" == req_id,* ]] && continue
    # Extract status: it is always one of COMPLETE or MISSING and appears
    # as a standalone word in the line.
    if echo "$line" | grep -qE '(,COMPLETE,|,COMPLETE$)'; then
      complete=$((complete + 1))
      total=$((total + 1))
    elif echo "$line" | grep -qE '(,MISSING,|,MISSING$)'; then
      total=$((total + 1))
    fi
  done < "$csv"
  echo "$complete $total"
}

# List req_ids with a given status from a database.csv file.
# Outputs lines of: req_id<TAB>requirement_text
list_reqs_by_status() {
  local csv="$1"
  local status="$2"
  while IFS= read -r line; do
    [[ "$line" == req_id,* ]] && continue
    if echo "$line" | grep -qE "(,${status},|,${status}\$)"; then
      local req_id
      req_id="$(echo "$line" | cut -d',' -f1)"
      # requirement_text is field 4; extract it simply
      local req_text
      req_text="$(echo "$line" | cut -d',' -f4)"
      echo "${req_id}	${req_text}"
    fi
  done < "$csv"
}

# --- extract database.csv at each ref -------------------------------------

TMPDIR_BASE="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_BASE"' EXIT

BASE_CSV="${TMPDIR_BASE}/base.csv"
HEAD_CSV="${TMPDIR_BASE}/head.csv"

# If HEAD_REF is a special value "WORKDIR", use the working directory copy
# (useful for testing and local runs).
if [[ "$HEAD_REF" == "WORKDIR" ]]; then
  cp "${REPO_ROOT}/${DATABASE_REL}" "$HEAD_CSV"
else
  git -C "$REPO_ROOT" show "${HEAD_REF}:${DATABASE_REL}" > "$HEAD_CSV" 2>/dev/null || {
    # Fall back to working directory if ref doesn't have the file
    cp "${REPO_ROOT}/${DATABASE_REL}" "$HEAD_CSV"
  }
fi

git -C "$REPO_ROOT" show "${BASE_REF}:${DATABASE_REL}" > "$BASE_CSV" 2>/dev/null || {
  echo "WARNING: Could not extract ${DATABASE_REL} from ${BASE_REF}. Using HEAD copy as base."
  cp "$HEAD_CSV" "$BASE_CSV"
}

# --- compute coverage ------------------------------------------------------

read -r BASE_COMPLETE BASE_TOTAL <<< "$(count_coverage "$BASE_CSV")"
read -r HEAD_COMPLETE HEAD_TOTAL <<< "$(count_coverage "$HEAD_CSV")"

if [[ "$BASE_TOTAL" -eq 0 ]]; then
  BASE_PCT="0.0"
else
  # Use awk for floating point
  BASE_PCT="$(awk "BEGIN { printf \"%.1f\", ($BASE_COMPLETE / $BASE_TOTAL) * 100 }")"
fi

if [[ "$HEAD_TOTAL" -eq 0 ]]; then
  HEAD_PCT="0.0"
else
  HEAD_PCT="$(awk "BEGIN { printf \"%.1f\", ($HEAD_COMPLETE / $HEAD_TOTAL) * 100 }")"
fi

DELTA_REQS=$((HEAD_COMPLETE - BASE_COMPLETE))
DELTA_PCT="$(awk "BEGIN { printf \"%.1f\", $HEAD_PCT - $BASE_PCT }")"

# Sign prefix for delta
if [[ "$DELTA_REQS" -gt 0 ]]; then
  DELTA_REQS_FMT="+${DELTA_REQS}"
  DELTA_PCT_FMT="+${DELTA_PCT}%"
elif [[ "$DELTA_REQS" -eq 0 ]]; then
  DELTA_REQS_FMT="0"
  DELTA_PCT_FMT="${DELTA_PCT}%"
else
  DELTA_REQS_FMT="${DELTA_REQS}"
  DELTA_PCT_FMT="${DELTA_PCT}%"
fi

# --- diff requirements -----------------------------------------------------

# Find reqs that are COMPLETE in head but not in base (gained coverage)
GAINED=""
while IFS=$'\t' read -r req_id req_text; do
  if ! grep -qE "^${req_id}," "$BASE_CSV" || \
     ! grep -E "^${req_id}," "$BASE_CSV" | grep -qE '(,COMPLETE,|,COMPLETE$)'; then
    GAINED="${GAINED}- ${req_id}: ${req_text}\n"
  fi
done < <(list_reqs_by_status "$HEAD_CSV" "COMPLETE")

# Find reqs that were COMPLETE in base but not in head (lost coverage)
LOST=""
while IFS=$'\t' read -r req_id req_text; do
  if ! grep -qE "^${req_id}," "$HEAD_CSV" || \
     ! grep -E "^${req_id}," "$HEAD_CSV" | grep -qE '(,COMPLETE,|,COMPLETE$)'; then
    LOST="${LOST}- ${req_id}: ${req_text}\n"
  fi
done < <(list_reqs_by_status "$BASE_CSV" "COMPLETE")

# --- format output ---------------------------------------------------------

COMMENT_BODY="## RTMX Requirements Coverage

| | Requirements | Percentage |
|---|---|---|
| Base (${BASE_REF}) | ${BASE_COMPLETE} / ${BASE_TOTAL} | ${BASE_PCT}% |
| This PR | ${HEAD_COMPLETE} / ${HEAD_TOTAL} | ${HEAD_PCT}% |
| **Delta** | **${DELTA_REQS_FMT}** | **${DELTA_PCT_FMT}** |"

if [[ -n "$GAINED" ]]; then
  COMMENT_BODY="${COMMENT_BODY}

### New requirements covered
$(echo -e "$GAINED" | sed '/^$/d')"
else
  COMMENT_BODY="${COMMENT_BODY}

### New requirements covered
(none)"
fi

if [[ -n "$LOST" ]]; then
  COMMENT_BODY="${COMMENT_BODY}

### Requirements lost coverage
$(echo -e "$LOST" | sed '/^$/d')"
else
  COMMENT_BODY="${COMMENT_BODY}

### Requirements lost coverage
(none)"
fi

echo "$COMMENT_BODY"

# --- post comment ----------------------------------------------------------

if [[ "$POST_COMMENT" == "true" ]]; then
  if [[ -z "${GITHUB_TOKEN:-}" && -z "${GH_TOKEN:-}" ]]; then
    echo ""
    echo "WARNING: --post-comment requested but no GITHUB_TOKEN or GH_TOKEN set. Skipping."
    exit 0
  fi

  PR_NUMBER="${PR_NUMBER:-}"
  if [[ -z "$PR_NUMBER" ]]; then
    # Try to detect PR number from GitHub Actions context
    PR_NUMBER="${GITHUB_EVENT_PULL_REQUEST_NUMBER:-}"
  fi
  if [[ -z "$PR_NUMBER" && -n "${GITHUB_REF:-}" ]]; then
    # Extract from refs/pull/123/merge
    PR_NUMBER="$(echo "$GITHUB_REF" | grep -oE 'pull/[0-9]+' | cut -d'/' -f2 || true)"
  fi

  if [[ -z "$PR_NUMBER" ]]; then
    echo ""
    echo "WARNING: Could not determine PR number. Skipping comment post."
    exit 0
  fi

  gh pr comment "$PR_NUMBER" --body "$COMMENT_BODY"
  echo ""
  echo "Posted coverage delta comment to PR #${PR_NUMBER}."
fi
