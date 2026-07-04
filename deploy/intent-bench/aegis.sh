#!/usr/bin/env bash
# aegis.sh — aegis (local-model, air-gapped) agent wrapper for intent-bench.
#
# Drop into an intent-bench checkout as agents/aegis.sh (see deploy/intent-bench/README.md).
# Drives `aegis run` (opencode serve-drive -> local model) in the experiment workdir and
# produces the transcript intent-bench scores.
#
# Usage (intent-bench agent contract):
#   aegis.sh <workdir> <model> <prompt_file> <result_dir> <max_budget>
#     - <model>      : the LOCAL model id (Ollama tag / served GGUF), NOT a cloud model.
#     - <max_budget> : a USD budget for cloud agents; local inference has no $ cost, so it
#                      is ignored — AEGIS_TIMEOUT bounds wall-clock instead.
#     - produces $result_dir/transcript.jsonl + $result_dir/stderr.log; exit 0 = completed.
#
# Control vs treatment: intent-bench seeds the workdir's .mcp.json (the rtmx MCP over the
# experiment's requirements) for the TREATMENT condition and leaves it absent for CONTROL.
# So aegis runs WITHOUT injecting its OWN intent layer (--no-intent); OpenCode picks up the
# workdir's .mcp.json when present. That keeps the treatment = intent-bench's requirements
# (not aegis's own repo), and control = no intent — exactly the A/B the benchmark intends.
#
# Env:
#   AEGIS_BIN       aegis binary (default: aegis on PATH)
#   AEGIS_ENDPOINT  OpenAI-compatible local endpoint (default: http://127.0.0.1:11434, Ollama;
#                   use http://127.0.0.1:<port> for a llama-server brought up via `aegis serve`)
#   AEGIS_TIMEOUT   per-run wall-clock budget (default: 3600s — local project builds are slow)
set -euo pipefail

workdir="${1:?Usage: aegis.sh <workdir> <model> <prompt_file> <result_dir> <max_budget>}"
model="${2:?model required}"
prompt_file="${3:?prompt_file required}"
result_dir="${4:?result_dir required}"
max_budget="${5:-0}" # accepted for interface parity; unused (local inference has no $ cost)

AEGIS="${AEGIS_BIN:-aegis}"
ENDPOINT="${AEGIS_ENDPOINT:-http://127.0.0.1:11434}"
TIMEOUT="${AEGIS_TIMEOUT:-3600s}"

# Make the interface paths absolute — aegis is run from its own root below (so it can find
# its bundled OpenCode/ripgrep/config-seed, which resolve relative to its dir/CWD).
workdir="$(cd "$workdir" && pwd)"
result_dir="$(cd "$result_dir" && pwd)"
prompt_file="$(cd "$(dirname "$prompt_file")" && pwd)/$(basename "$prompt_file")"

# A packaged aegis (or one given $AEGIS_LIBEXEC) resolves its bundled OpenCode/ripgrep/
# config-seed/llama-server via the install libexec (REL-005) — no cd needed. In a source
# tree (no libexec), cd into the aegis root so its cwd-relative deploy/opencode/* resolve.
if [ -z "${AEGIS_LIBEXEC:-}" ]; then
    AEGIS_ROOT="${AEGIS_ROOT:-$(cd "$(dirname "$AEGIS")/.." 2>/dev/null && pwd || true)}"
    if [ -n "${AEGIS_ROOT:-}" ] && [ -d "$AEGIS_ROOT/deploy/opencode" ]; then
        cd "$AEGIS_ROOT"
    fi
fi

cfg="$result_dir/aegis.json"
printf '{"endpoint":"%s","harness":"opencode","model_id":"%s","target":"linux-cpu","allow_egress":false,"audit_path":"%s/audit.log"}\n' \
    "$ENDPOINT" "$model" "$result_dir" > "$cfg"

# --no-intent: do NOT inject aegis's own rtmx layer. The benchmark controls intent via the
# workdir (.mcp.json seeded for treatment, absent for control), keeping the A/B clean.
exec "$AEGIS" run --no-intent \
    --config "$cfg" \
    --workdir "$workdir" \
    --model "$model" \
    --prompt-file "$prompt_file" \
    --timeout "$TIMEOUT" \
    --out "$result_dir/transcript.jsonl" \
    2> "$result_dir/stderr.log"
