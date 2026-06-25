#!/usr/bin/env bash
# bakeoff.sh — SERVE-016 screening: run `aegis run` on a tiny tool-calling task
# across candidate local models, recording completion + wall-clock. The screening
# task ("create a file with exact contents") is the minimum agentic bar — it
# requires the model to emit a real write tool call (not prose).
set -u
REPO="$(cd "$(dirname "$0")/.." && pwd)"; cd "$REPO"
OUT="${BAKEOFF_OUT:-/tmp/bakeoff}"; mkdir -p "$OUT"
RESULTS="$OUT/results.csv"; echo "model,seconds,rc,pass,bytes" > "$RESULTS"
TASK="Create a file named hello.txt in the current directory containing exactly: hello world"
MODELS="${BAKEOFF_MODELS:-laguna-xs.2:latest phi4-mini:latest gemma4:e4b gemma4-qat:32k gemma4:26b}"
for m in $MODELS; do
  safe="$(printf '%s' "$m" | tr '/:' '__')"
  d="$OUT/$safe"; mkdir -p "$d"; rm -f "$d/hello.txt"
  printf '{"endpoint":"http://127.0.0.1:11434","model_id":"%s","harness":"builtin","allow_egress":false,"target":"linux-cpu"}\n' "$m" > "$d/cfg.json"
  start=$(date +%s)
  timeout 320 "$REPO/bin/aegis" run --workdir "$d" --config "$d/cfg.json" --model "$m" \
    --timeout 300s --prompt "$TASK" --out "$d/transcript.jsonl" >"$d/out" 2>"$d/err"
  rc=$?; end=$(date +%s)
  pass=no; bytes=0
  if [ -f "$d/hello.txt" ]; then bytes=$(wc -c <"$d/hello.txt"); grep -qi 'hello world' "$d/hello.txt" && pass=YES; fi
  echo "$m,$((end-start)),$rc,$pass,$bytes" >> "$RESULTS"
  echo "bakeoff: $m -> $((end-start))s rc=$rc pass=$pass" >&2
done
echo "bakeoff: done -> $RESULTS" >&2
