#!/usr/bin/env bash
# aegis.sh — intent-bench system-under-test adapter (BENCH-003).
#
# Conforms to the intent-bench agent contract:
#   agents/<name>.sh <workdir> <model> <prompt_file> <result_dir> <max_budget>
#
# Drives `aegis run` (headless OpenCode on the local model) and writes
# <result_dir>/transcript.jsonl (intent-bench NDJSON, via internal/bench) +
# <result_dir>/stderr.log. Exit 0 = the agent completed, non-zero = it crashed.
#
# Wire it into intent-bench by symlinking it as agents/aegis.sh, then:
#   bash bench.sh run <experiment> --agent aegis --model <local-id> ...
set -u
workdir="$1"
model="$2"
prompt_file="$3"
result_dir="$4"
max_budget="${5:-}" # intent-bench passes a USD budget; informational for a free
                    # local model (we still emit token counts). Map to wall-clock.
mkdir -p "$result_dir"

# Locate the aegis binary (env override, PATH, or alongside this checkout).
AEGIS="${AEGIS_BIN:-}"
if [ -z "$AEGIS" ]; then
	if command -v aegis >/dev/null 2>&1; then AEGIS=aegis
	else AEGIS="$(cd "$(dirname "$0")/../.." && pwd)/bin/aegis"; fi
fi

# Config: AEGIS_CONFIG points aegis at the local serving endpoint (loopback).
cfg_args=""
[ -n "${AEGIS_CONFIG:-}" ] && cfg_args="--config ${AEGIS_CONFIG}"
timeout="${INTENT_BENCH_TIMEOUT:-10m}"

# Control vs treatment (BENCH-004): intent-bench seeds the treatment into the
# workdir (treatments/rtmx.sh creates .intent-bench/ + .mcp.json). CONTROL runs
# omit the rtmx MCP intent layer (--no-intent) so intent-tool tokens are zero (the
# intent-bench ledger rule). AEGIS_INTENT={0,1} forces it.
intent_args=""
case "${AEGIS_INTENT:-auto}" in
0) intent_args="--no-intent" ;;
1) intent_args="" ;;
*) { [ -d "$workdir/.intent-bench" ] || [ -f "$workdir/.mcp.json" ]; } || intent_args="--no-intent" ;;
esac

# shellcheck disable=SC2086
"$AEGIS" run \
	--workdir "$workdir" \
	--model "$model" \
	--prompt-file "$prompt_file" \
	--timeout "$timeout" \
	$cfg_args $intent_args \
	--out "$result_dir/transcript.jsonl" \
	2>"$result_dir/stderr.log"
exit $?
