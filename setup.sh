#!/usr/bin/env bash
# setup.sh — one-command host bring-up for the full aegis stack (REL-004).
#
# Run this on the CONNECTED build host. It builds the whole stack from pinned
# source (aegis + OpenCode + llama.cpp), stages + verifies the model GGUF,
# calibrates the serving to this host, and runs the full-stack integration smoke.
# Then carry deploy/{opencode,llama-server,models}/ + the aegis binary into the
# enclave (see docs/operator-guide.md).
#
# It is guided + idempotent: each phase prints what it did, and model-dependent
# phases skip with clear guidance until a GGUF is staged. Honors the air-gap
# posture — nothing here fetches at runtime; only the build phase touches the
# network (pinned source + frozen deps).
set -eu
cd "$(dirname "$0")"

step() { printf '\n=== setup: %s ===\n' "$1"; }
model_name() { grep -o '"name"[^,]*' deploy/models/MODEL_REF 2>/dev/null | sed 's/.*: *"//; s/".*//'; }

# 0. Prerequisites.
step "checking toolchain"
missing=0
for t in go bun cmake cc git; do
	if ! command -v "$t" >/dev/null 2>&1; then echo "  MISSING: $t" >&2; missing=1; fi
done
if [ "$missing" != 0 ]; then
	echo "setup: install the missing tools first — Bun (https://bun.sh), Go 1.25.x, a C/C++ toolchain (cmake + cc)." >&2
	exit 1
fi
echo "  toolchain OK"

# 1. Build the full stack (aegis + OpenCode + llama-server) from pinned source.
step "building the full stack (make ci-full)"
make ci-full

# 2. Stage + verify the model GGUF (honors the deploy/models/MODEL_REF pin).
step "staging the model"
if [ -n "${MODEL_SRC:-}" ]; then
	scripts/stage-model.sh
else
	echo "  skipped: set MODEL_SRC=<dir containing the pinned GGUF> to stage the model." >&2
	echo "  first finalize the SERVE-016 bake-off winner + its sha256 in deploy/models/MODEL_REF." >&2
fi

model_path="deploy/models/$(model_name)"

# 3. Calibrate the serving to this host (writes deploy/llama-server/calibration.json).
step "calibrating the serving"
if [ -f "$model_path" ]; then
	scripts/bench.sh --model "$model_path" || echo "  calibration skipped/failed (see scripts/bench.sh)." >&2
else
	echo "  skipped: stage a model first (phase 2)." >&2
fi

# 4. Full-stack integration smoke (brings the stack up on loopback; asserts EGRESS=0).
step "integration smoke"
if [ -f "$model_path" ]; then
	scripts/integration-smoke.sh
else
	echo "  skipped: needs a staged model." >&2
fi

step "done"
echo "setup: stack built under deploy/{opencode,llama-server,models}/ + ./bin/aegis."
echo "setup: next — install + run in the enclave per docs/operator-guide.md."
