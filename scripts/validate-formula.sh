#!/usr/bin/env bash
# validate-formula.sh <formula.rb> — REL-014: fail if a generated Homebrew formula is not loadable.
# Homebrew rejects a formula whose active spec has no url ("formula requires at least a URL"), and
# that error takes down `brew upgrade` for the ENTIRE tap — so an empty on_macos/on_linux block, a
# leftover REPLACE_ placeholder, or a formula with no url at all must fail the RELEASE, never ship.
# Run at release time (invoked by fill-formula.sh) and covered by test::TestFormulaValidation.
set -eu
f="${1:?usage: validate-formula.sh <formula.rb>}"
fail() { echo "validate-formula: INVALID ($f) — $1" >&2; exit 1; }

grep -q "REPLACE_" "$f" && fail "leftover REPLACE_ placeholder (an unbuilt platform was not pruned)"
grep -qE '^[[:space:]]*url ' "$f" || fail "no url anywhere in the formula"

# Every declared on_macos/on_linux wrapper must contain a url — an empty OS block is the exact
# breakage (evaluates on that OS to no spec). Depth-tracked so inner `end`s don't close it early.
awk '
/^[[:space:]]*on_(macos|linux) do[[:space:]]*$/ && !inos { os=$0; inos=1; depth=1; hasurl=0; next }
inos {
  if ($0 ~ /[[:space:]]url /) hasurl=1
  if ($0 ~ /[[:space:]]do[[:space:]]*$/) depth++
  if ($0 ~ /^[[:space:]]*end[[:space:]]*$/) { depth--; if (depth==0) { if (!hasurl) { print "  empty block: " os; bad=1 } inos=0 } }
  next
}
END { exit bad ? 1 : 0 }
' "$f" || fail "an on_macos/on_linux block has no url (breaks brew: 'formula requires at least a URL')"

# REL-014: the formula must serve EVERY supported platform. A formula missing a platform's url cannot
# LOAD on that platform (macOS with only linux urls errors "requires at least a URL" too) — so a partial
# build (e.g. darwin failed) must FAIL the release, not silently publish a formula that breaks a platform.
required="${AEGIS_REQUIRED_PLATFORMS:-darwin-arm64 linux-arm64 linux-amd64}"
for plat in $required; do
	grep -qF "$plat.tar.gz" "$f" || fail "missing url for required platform '$plat' — incomplete release (would break brew on that platform)"
done

echo "validate-formula: OK ($f) — all required platforms present [$required]" >&2
