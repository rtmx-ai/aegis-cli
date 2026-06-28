#!/usr/bin/env bash
# integration-smoke.sh — full-stack integration smoke (BUILD-012, gated): bring the
# whole stack up on loopback — llama-server (calibrated, --jinja) + the pinned model +
# OpenCode — and drive `aegis run` on a tiny real task, asserting it completes under the
# egress gate (EGRESS=0). This exercises the SAME path a user gets: aegis → opencode
# serve-drive → llama-server → model. Requires the built stack (`make ci-full`) + a model
# GGUF (MODEL_OUT or a staged deploy/models/<MODEL_REF name>); exits with guidance if missing.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"

# AEGIS_BIN / AEGIS_LIBEXEC let the closed-host smoke (ENCLAVE-003, scripts/enclave-smoke.sh)
# drive an INSTALLED package instead of the source tree: point AEGIS at the installed binary
# and resolve the helpers from its libexec. Default is the source-tree layout (BUILD-012).
AEGIS="${AEGIS_BIN:-./bin/aegis}"
if [ -n "${AEGIS_LIBEXEC:-}" ]; then
	export AEGIS_LIBEXEC
	LLAMA="$AEGIS_LIBEXEC/llama-server"; OPENCODE="$AEGIS_LIBEXEC/opencode"
else
	LLAMA="deploy/llama-server/bin/llama-server"; OPENCODE="deploy/opencode/bin/opencode"
fi
for f in "$AEGIS" "$LLAMA" "$OPENCODE"; do
	[ -x "$f" ] || { echo "integration-smoke: missing $f — run 'make ci-full' (or 'make build') first." >&2; exit 1; }
done
model="$(grep -o '"name"[^,]*' deploy/models/MODEL_REF | sed 's/.*: *"//; s/".*//')"
staged="${MODEL_OUT:-deploy/models/$model}"
[ -f "$staged" ] || { echo "integration-smoke: model not found at $staged — stage it (scripts/stage-model.sh) or set MODEL_OUT to a local GGUF." >&2; exit 1; }

port="${SMOKE_PORT:-8091}"
threads=$(nproc); [ "$threads" -gt 16 ] && threads=16

# Bring the model server up under the calibrated launch args (taskset/nice, --jinja,
# --ctx-size) via `aegis serve` — the real production launch path (SERVE-017).
cal="$(mktemp)"
printf '{"target":"linux-cpu","threads":%s,"batch":512,"ngl":0,"model":"%s","port":%s,"ctx_size":16384}\n' \
	"$threads" "$staged" "$port" > "$cal"
"$AEGIS" serve --calibration "$cal" >/tmp/llama-smoke.log 2>&1 &
LL=$!
trap 'kill $LL 2>/dev/null; rm -f "$cal"' EXIT
echo "integration-smoke: bringing up llama-server (model=$model, port=$port, --jinja)…"
up=no
for _ in $(seq 1 60); do
	curl -s -m2 "http://127.0.0.1:$port/health" 2>/dev/null | grep -qi '"status":"ok"' && { up=yes; break; }
	sleep 2
done
[ "$up" = yes ] || { echo "integration-smoke: llama-server did not become healthy (see /tmp/llama-smoke.log)" >&2; exit 1; }

# Drive a tiny real task through the full stack under the egress gate (EGRESS=0 must hold).
# Use the reliable pattern (edit an EXISTING file in the workdir, explicit tool) so the smoke
# is deterministic — it validates stack integration, not a model's tool-call luck.
work="$(mktemp -d)"
printf 'REPLACE_ME\n' > "$work/greeting.txt"
printf '{"endpoint":"http://127.0.0.1:%s","model_id":"%s","harness":"opencode","allow_egress":false,"target":"linux-cpu"}\n' \
	"$port" "$model" > "$work/aegis.json"
# The egress gate wraps the run by default; the closed-host smoke (enclave-smoke.sh) sets
# ENCLAVE_OUTER_GATE since it already runs the whole flow under verify-airgap / an offline host
# (nesting unshare -rn would fail).
RUN_WRAP="scripts/verify-airgap.sh --"; [ -n "${ENCLAVE_OUTER_GATE:-}" ] && RUN_WRAP=""
$RUN_WRAP "$AEGIS" run --workdir "$work" --config "$work/aegis.json" \
	--timeout "${SMOKE_TIMEOUT:-300s}" \
	--prompt "Use the edit/write tool to change the file greeting.txt in the working directory so its entire contents are exactly the two words: hello world (replacing REPLACE_ME). Then you are done." \
	--out "$work/transcript.jsonl"

if [ -f "$work/greeting.txt" ] && grep -qi "hello world" "$work/greeting.txt"; then
	echo "integration-smoke: PASS — full stack (aegis + llama-server + model + opencode) completed the task (EGRESS=0)"
else
	echo "integration-smoke: FAIL — task did not complete; see $work/transcript.jsonl" >&2
	exit 1
fi
