#!/usr/bin/env bash
# fill-formula.sh <version> <dist-dir> [template] [out] — produce a PINNED Homebrew formula
# from the template (deploy/homebrew/aegis.rb) by setting `version` and each platform's
# sha256 from the release bundle tarballs dist/aegis-<version>-<os>-<arch>.tar.gz. URLs use
# Ruby #{version} interpolation, so only version + the sha256 placeholders are filled.
# Platforms whose tarball is absent (built on another host) keep the REPLACE_ placeholder.
# REL-007 — used by the release workflow + test::TestTapFormulaPinned.
set -eu
version="${1:?usage: fill-formula.sh <version> <dist-dir> [template] [out]}"
dist="${2:?dist dir required}"
template="${3:-deploy/homebrew/aegis.rb}"
out="${4:-/dev/stdout}"

sha_of() {
	[ -f "$1" ] || return 1
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
	else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

tmp="$(mktemp)"; cp "$template" "$tmp"
sed -i.bak "s/version \"0.0.0\"/version \"$version\"/" "$tmp" && rm -f "$tmp.bak"
for plat in darwin-arm64 darwin-amd64 linux-arm64 linux-amd64; do
	tarball="$dist/aegis-$version-$plat.tar.gz"
	sha="$(sha_of "$tarball" || true)"
	[ -n "$sha" ] || continue
	ph="REPLACE_$(printf '%s' "$plat" | tr 'a-z-' 'A-Z_')_SHA256"
	sed -i.bak "s/$ph/$sha/" "$tmp" && rm -f "$tmp.bak"
	echo "fill-formula: pinned $plat -> $sha" >&2
done
# Pass 1: prune any inner on_arm/on_intel block whose sha256 is still a REPLACE_ placeholder
# (that tarball was not built on this runner) — a placeholder sha256 breaks brew on that platform.
p1="$(mktemp)"
awk '
/^[[:space:]]*on_(arm|intel) do[[:space:]]*$/ { buf=$0 ORS; inblk=1; repl=0; next }
inblk {
  buf=buf $0 ORS
  if ($0 ~ /REPLACE_[A-Z0-9_]+_SHA256/) repl=1
  if ($0 ~ /^[[:space:]]*end[[:space:]]*$/) { if (!repl) printf "%s", buf; inblk=0; buf="" }
  next
}
{ print }
' "$tmp" > "$p1"

# Pass 2 (REL-014): drop an on_macos/on_linux wrapper left EMPTY by pass 1 (all inner blocks
# pruned). An OS block with no url is what breaks brew ("formula requires at least a URL") and
# takes down `brew upgrade` for the whole tap. Depth-tracked so inner `end`s do not close it early.
p2="$(mktemp)"
awk '
/^[[:space:]]*on_(macos|linux) do[[:space:]]*$/ && !inos { buf=$0 ORS; inos=1; depth=1; hasurl=0; next }
inos {
  buf=buf $0 ORS
  if ($0 ~ /[[:space:]]url /) hasurl=1
  if ($0 ~ /[[:space:]]do[[:space:]]*$/) depth++
  if ($0 ~ /^[[:space:]]*end[[:space:]]*$/) { depth--; if (depth==0) { if (hasurl) printf "%s", buf; inos=0; buf="" } }
  next
}
{ print }
' "$p1" > "$p2"
rm -f "$tmp" "$p1"

# REL-014: validate the generated formula is loadable BEFORE it is published — fails the release
# on a leftover placeholder, no url, or an empty on_macos/on_linux block.
"$(dirname "$0")/validate-formula.sh" "$p2"
cat "$p2" > "$out"
rm -f "$p2"
