#!/usr/bin/env bash
# integration-smoke.sh — full-stack integration smoke (BUILD-012, gated): bring the
# stack up on loopback (llama-server + the staged model) and run `aegis run` on a
# tiny task, asserting EGRESS=0. Requires `make ci-full` + a staged model; exits
# with clear guidance if prerequisites are missing.
set -eu
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"; REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"; cd "$REPO_ROOT"
LLAMA="deploy/llama-server/bin/llama-server"
OPENCODE="deploy/opencode/bin/opencode"
for f in "$LLAMA" "$OPENCODE"; do
	[ -x "$f" ] || { echo "integration-smoke: missing $f — run 'make ci-full' first." >&2; exit 1; }
done
model="$(grep -o '"name"[^,]*' deploy/models/MODEL_REF | sed 's/.*: *"//; s/".*//')"
staged="${MODEL_OUT:-deploy/models/$model}"
[ -f "$staged" ] || { echo "integration-smoke: model $staged not staged — run scripts/stage-model.sh." >&2; exit 1; }

# Launch llama-server under the calibrated args (loopback), wait for health.
port=8080
"$LLAMA" --model "$staged" --host 127.0.0.1 --port "$port" >/tmp/llama-smoke.log 2>&1 &
LL=$!; trap 'kill $LL 2>/dev/null' EXIT
for _ in $(seq 1 60); do curl -sf "http://127.0.0.1:$port/health" >/dev/null 2>&1 && break; sleep 1; done

# Run the keystone task under the egress gate (EGRESS=0 must hold across the group).
work="$(mktemp -d)"
printf '{"endpoint":"http://127.0.0.1:%s","model_id":"%s","harness":"builtin","allow_egress":false,"target":"linux-cpu"}\n' "$port" "$model" > "$work/aegis.json"
scripts/verify-airgap.sh -- ./bin/aegis run --workdir "$work" --config "$work/aegis.json" \
	--timeout 240s --prompt "Create hello.txt containing exactly: hello world" --out "$work/transcript.jsonl"
if [ -f "$work/hello.txt" ]; then
	echo "integration-smoke: PASS — full stack completed the task (EGRESS=0)"
else
	echo "integration-smoke: task did not complete (see $work/transcript.jsonl)"; exit 1
fi
