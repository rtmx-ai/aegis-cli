#!/usr/bin/env bash
# run-suite.sh — run the real intent-bench against aegis with ONE local model, both conditions.
#
# Brings the model up via `aegis serve` (llama-server, loopback only), runs the given experiments
# under CONTROL (no intent) and the rtmx TREATMENT, then tears the server down. Run it once PER
# MODEL to get separate per-model results — intent-bench's results/summary.csv tags every row by
# the --model id, so two models land as distinguishable rows for `make analyze` + per-model PRs
# (REQ-BENCH-009). Each model is served in isolation (one GGUF at a time; bandwidth-bound on CPU).
#
# Usage:
#   deploy/intent-bench/run-suite.sh \
#       --bench /path/to/intent-bench --gguf ~/models/<model>.gguf --model-id <label> \
#       [--experiments "url-shortener task-manager ..."] [--runs N] [--port P] [--timeout 3600s]
#
# Example — two models, separate runs:
#   run-suite.sh --bench ../../intent-bench --gguf ~/models/gemma-4-26B-A4B-...gguf --model-id gemma-4-26b-a4b
#   run-suite.sh --bench ../../intent-bench --gguf ~/models/Qwen3-Coder-30B-...gguf --model-id qwen3-coder-30b
set -euo pipefail
SELF="$(cd "$(dirname "$0")" && pwd)"; AEGIS_ROOT_DIR="$(cd "$SELF/../.." && pwd)"

BENCH="" GGUF="" MODEL_ID="" RUNS=5 PORT=8090 TIMEOUT="3600s"
# Default to the six "claim" experiments documented in intent-bench/REPRODUCING.md.
EXPERIMENTS="url-shortener task-manager rest-api cli-tool brownfield rtmx-self"
while [ $# -gt 0 ]; do
	case "$1" in
		--bench) BENCH="$2"; shift 2;;
		--gguf) GGUF="$2"; shift 2;;
		--model-id) MODEL_ID="$2"; shift 2;;
		--experiments) EXPERIMENTS="$2"; shift 2;;
		--runs) RUNS="$2"; shift 2;;
		--port) PORT="$2"; shift 2;;
		--timeout) TIMEOUT="$2"; shift 2;;
		*) echo "run-suite: unknown arg: $1" >&2; exit 2;;
	esac
done
[ -n "$BENCH" ] && [ -n "$GGUF" ] && [ -n "$MODEL_ID" ] || { echo "run-suite: need --bench, --gguf, --model-id" >&2; exit 2; }
BENCH="$(cd "$BENCH" && pwd)"
[ -f "$GGUF" ] || { echo "run-suite: GGUF not found: $GGUF" >&2; exit 1; }

AEGIS="${AEGIS_BIN:-$AEGIS_ROOT_DIR/bin/aegis}"; [ -x "$AEGIS" ] || AEGIS="aegis"
threads=$(nproc); [ "$threads" -gt 16 ] && threads=16

# 1. Bring the model up via aegis serve (the production launch path: calibrated, --jinja, loopback).
cal="$(mktemp)"
printf '{"target":"linux-cpu","threads":%s,"batch":512,"ngl":0,"model":"%s","port":%s,"ctx_size":16384}\n' \
	"$threads" "$GGUF" "$PORT" > "$cal"
echo "run-suite: bringing up $MODEL_ID on port $PORT…" >&2
( cd "$AEGIS_ROOT_DIR" && exec "$AEGIS" serve --calibration "$cal" ) >"/tmp/intent-bench-serve-$MODEL_ID.log" 2>&1 &
SERVE=$!
trap 'kill "$SERVE" 2>/dev/null || true; rm -f "$cal"' EXIT
up=no
for _ in $(seq 1 90); do
	curl -s -m2 "http://127.0.0.1:$PORT/health" 2>/dev/null | grep -qi '"status":"ok"' && { up=yes; break; }
	sleep 2
done
[ "$up" = yes ] || { echo "run-suite: model server not healthy (see /tmp/intent-bench-serve-$MODEL_ID.log)" >&2; exit 1; }
echo "run-suite: $MODEL_ID healthy." >&2

# 2. Wire agents/aegis.sh to THIS aegis + endpoint. (aegis.sh cds to AEGIS_ROOT when no libexec.)
export AEGIS_BIN="$AEGIS" AEGIS_ENDPOINT="http://127.0.0.1:$PORT" AEGIS_TIMEOUT="$TIMEOUT"
[ -n "${AEGIS_LIBEXEC:-}" ] || export AEGIS_ROOT="$AEGIS_ROOT_DIR"

# 3. Run each experiment under control + rtmx treatment (a row per run, tagged model=$MODEL_ID).
cd "$BENCH"
[ -f results/summary.csv ] || bash bench.sh init-ledger
for exp in $EXPERIMENTS; do
	echo "run-suite: [$MODEL_ID] $exp — control (N=$RUNS)…" >&2
	bash bench.sh run "$exp" --condition control --runs "$RUNS" --agent aegis --model "$MODEL_ID" \
		|| echo "run-suite: [$MODEL_ID] $exp control reported failures (continuing)" >&2
	echo "run-suite: [$MODEL_ID] $exp — treatment/rtmx (N=$RUNS)…" >&2
	bash bench.sh run "$exp" --condition treatment --treatment rtmx --runs "$RUNS" --agent aegis --model "$MODEL_ID" \
		|| echo "run-suite: [$MODEL_ID] $exp treatment reported failures (continuing)" >&2
done
echo "run-suite: $MODEL_ID DONE — rows in $BENCH/results/summary.csv (model=$MODEL_ID). Next: make analyze + open a per-model PR." >&2
