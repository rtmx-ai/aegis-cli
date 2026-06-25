#!/usr/bin/env bash
# fetch-model.sh — download + sha256-verify a catalog model (MODEL-003). Runs on
# the CONNECTED build host only (the enclave never fetches). Prints the downloaded
# path on stdout; progress + messages go to stderr. Idempotent: re-uses an existing
# verified file.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"
id="${1:?usage: fetch-model.sh <catalog-id>}"
DEST="${MODEL_DOWNLOAD_DIR:-$HOME/models}"
mkdir -p "$DEST"
hash() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

eval "$(python3 -c "
import json,sys
for m in json.load(open('deploy/models/catalog.json'))['models']:
    if m['id']=='$id':
        print('url=%r; file=%r; sha=%r; size=%d' % (m['url'], m['file'], m.get('sha256',''), m.get('size',0))); break
else: sys.exit('fetch-model: unknown catalog id: $id (see deploy/models/catalog.json)')
")"
out="$DEST/$file"

if [ -f "$out" ] && { [ -z "$sha" ] || [ "$(hash "$out")" = "$sha" ]; }; then
	echo "fetch-model: already present + verified: $out" >&2
else
	echo "fetch-model: downloading $file (~$((size / 1024 / 1024 / 1024)) GB) -> $out" >&2
	curl -fL --progress-bar -o "$out.part" "$url"
	mv "$out.part" "$out"
	if [ -n "$sha" ]; then
		got="$(hash "$out")"
		[ "$got" = "$sha" ] || {
			echo "fetch-model: sha256 MISMATCH for $file (want $sha, got $got)" >&2
			rm -f "$out"
			exit 1
		}
		echo "fetch-model: sha256 verified" >&2
	fi
fi
printf '%s\n' "$out"
