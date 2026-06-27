#!/usr/bin/env bash
# build-apt-repo.sh <dist-dir> <out-dir> [suite] — generate a flat-ish apt repository from
# the .deb packages in <dist-dir> for hosting on GitHub Pages (REL-007):
#   <out>/pool/main/aegis_<v>_<arch>.deb
#   <out>/dists/<suite>/main/binary-<arch>/Packages[.gz]
#   <out>/dists/<suite>/Release  (+ InRelease/Release.gpg if a GPG key is available)
# apt's trust anchor is a GPG-signed Release; sign it with the GPG_KEY secret (the minisign
# key covers the release SHA256SUMS, a separate concern). Unsigned is built with a loud NOTE.
set -eu
dist="${1:?usage: build-apt-repo.sh <dist-dir> <out-dir> [suite]}"
out="${2:?out dir required}"
suite="${3:-stable}"
command -v dpkg-scanpackages >/dev/null 2>&1 || { echo "build-apt-repo: dpkg-scanpackages (dpkg-dev) required" >&2; exit 2; }

rm -rf "$out"; mkdir -p "$out/pool/main"
debs="$(ls "$dist"/aegis_*.deb 2>/dev/null || true)"
[ -n "$debs" ] || { echo "build-apt-repo: no .deb in $dist" >&2; exit 1; }
cp $debs "$out/pool/main/"

arches="$(for d in "$out"/pool/main/*.deb; do dpkg-deb -f "$d" Architecture; done | sort -u)"
for arch in $arches; do
	bd="$out/dists/$suite/main/binary-$arch"; mkdir -p "$bd"
	( cd "$out" && dpkg-scanpackages --arch "$arch" pool/main > "dists/$suite/main/binary-$arch/Packages" 2>/dev/null )
	gzip -9c "$bd/Packages" > "$bd/Packages.gz"
done

# Release file (apt metadata over the Packages indices).
rel="$out/dists/$suite/Release"
{
	echo "Origin: aegis-cli"
	echo "Label: aegis-cli"
	echo "Suite: $suite"
	echo "Codename: $suite"
	echo "Architectures: $(echo $arches | tr '\n' ' ')"
	echo "Components: main"
	echo "Date: $(date -u '+%a, %d %b %Y %H:%M:%S UTC')"
	echo "SHA256:"
	( cd "$out/dists/$suite" && find main -type f | while read -r f; do
		printf ' %s %16d %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$(stat -c%s "$f")" "$f"
	done )
} > "$rel"

if command -v gpg >/dev/null 2>&1 && [ -n "${GPG_KEY:-}" ]; then
	gpg --batch --yes --local-user "$GPG_KEY" -abs -o "$out/dists/$suite/Release.gpg" "$rel"
	gpg --batch --yes --local-user "$GPG_KEY" --clearsign -o "$out/dists/$suite/InRelease" "$rel"
	echo "build-apt-repo: signed Release (GPG $GPG_KEY)" >&2
else
	echo "build-apt-repo: NOTE — Release is UNSIGNED (set GPG_KEY for a trusted apt repo)" >&2
fi
echo "$out"
