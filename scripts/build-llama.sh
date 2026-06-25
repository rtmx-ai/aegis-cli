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

# Pinned source (shallow at the ref — faster, less data to stage for air-gap).
if [ ! -d "$SRC/.git" ]; then
	git clone --depth 1 --branch "$REF" "$SOURCE_REPO" "$SRC"
else
	git -C "$SRC" fetch --depth 1 origin "$REF"
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
