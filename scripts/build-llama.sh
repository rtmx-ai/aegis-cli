#!/usr/bin/env bash
# build-llama.sh — build the production llama.cpp `llama-server` from pinned source
# (SERVE-017). The production serving path: no telemetry, no runtime network
# (LLAMA_CURL=OFF), target-aware (native CPU opts on linux-cpu; Metal on
# darwin-metal). Runs on the connected build host; the binary is then served
# air-gapped under the calibrated args from deploy/llama-server/calibration.json
# (internal/serving.LaunchArgs).
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# retry runs a command up to 3 times with exponential backoff (REL-013): a transient git fetch flake
# on a CI runner must not sink a release build.
retry() {
	local n=1 max=3 delay=5
	until "$@"; do
		if [ "$n" -ge "$max" ]; then
			echo "build-llama: '$*' failed after $max attempts" >&2
			return 1
		fi
		echo "build-llama: '$1 …' failed (attempt $n/$max); retrying in ${delay}s" >&2
		sleep "$delay"; n=$((n + 1)); delay=$((delay * 2))
	done
}

REFFILE="deploy/llama-server/LLAMA_REF"
REF="${LLAMA_REF:-$(tr -d ' \n' <"$REFFILE" 2>/dev/null || echo master)}"
SRC="${LLAMA_SRC:-build/llama.cpp}"
OUT="deploy/llama-server/bin"
SOURCE_REPO="https://github.com/ggml-org/llama.cpp"
mkdir -p "$OUT"

if ! command -v cmake >/dev/null 2>&1; then
	echo "build-llama: NOTE — cmake not installed; cannot build here." >&2
	echo "build-llama: install cmake + a C/C++ toolchain on the connected build host, then re-run." >&2
	exit 0
fi
if [ "$REF" = "master" ]; then
	echo "build-llama: WARNING — building from 'master' (not reproducible). Pin a concrete" >&2
	echo "build-llama: llama.cpp release tag in $REFFILE for an auditable build." >&2
fi

# Pinned source (shallow at the ref — faster, less data to stage for air-gap). Clone with retry +
# clean between attempts so a partial clone from a runner flake can't wedge the build (REL-013).
if [ ! -d "$SRC/.git" ]; then
	n=1
	until git clone --depth 1 --branch "$REF" "$SOURCE_REPO" "$SRC"; do
		[ "$n" -ge 3 ] && { echo "build-llama: clone failed after 3 attempts" >&2; exit 1; }
		echo "build-llama: clone attempt $n failed; cleaning + retrying" >&2
		rm -rf "$SRC"; sleep $((5 * n)); n=$((n + 1))
	done
else
	retry git -C "$SRC" fetch --depth 1 origin "$REF"
	git -C "$SRC" checkout -q FETCH_HEAD
fi
echo "build-llama: building llama.cpp @ $REF"

# Target-aware, air-gapped build flags.
EXTRA="-DGGML_NATIVE=ON" # linux-cpu: native CPU opts (AVX2/AVX-512 as available)
if [ "$(uname -s)" = "Darwin" ]; then
	EXTRA="-DGGML_METAL=ON" # darwin-metal: GPU offload
fi
cmake -S "$SRC" -B "$SRC/build" \
	-DCMAKE_BUILD_TYPE=Release \
	-DLLAMA_CURL=OFF \
	-DGGML_OPENMP=ON \
	-DBUILD_SHARED_LIBS=OFF \
	$EXTRA
cmake --build "$SRC/build" --target llama-server -j"$(nproc 2>/dev/null || echo 4)"

built="$(find "$SRC/build" -name llama-server -type f -perm -u+x | head -1)"
if [ -z "$built" ]; then
	echo "build-llama: build produced no llama-server binary" >&2
	exit 1
fi
install -m 0755 "$built" "$OUT/llama-server"
echo "build-llama: built $OUT/llama-server from $REF (no curl/telemetry, target=$(uname -s))"
