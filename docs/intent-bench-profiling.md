# Profiling aegis on intent-bench (local-only vs hosted Sonnet-4)

How to profile the **local-only aegis stack** (OpenCode + a local model + the rtmx
intent layer) against **hosted Claude Sonnet 4** on
[`intent-bench`](https://github.com/intent-bench/intent-bench). Requirements:
`BENCH-001..005`. Spec: `docs/requirements/intent-bench.md`.

## 0. Prereqs

- A working aegis stack on a serving host (`./setup.sh` with a staged model; the
  model must actually complete tasks — see RUNQ-004).
- An intent-bench checkout.
- `aegis` on PATH (or `AEGIS_BIN`), and `AEGIS_CONFIG` pointing at the local
  serving endpoint (loopback `:8080`).

## 1. Install the adapter

aegis ships an intent-bench SUT adapter (`scripts/intent-bench/aegis.sh`,
BENCH-003) that conforms to the contract
`agents/<name>.sh <workdir> <model> <prompt_file> <result_dir> <max_budget>` and
emits `transcript.jsonl` (intent-bench NDJSON) + `stderr.log`:

```bash
ln -s "$(pwd)/scripts/intent-bench/aegis.sh" <intent-bench>/agents/aegis.sh
export AEGIS_CONFIG=/path/to/aegis.json     # endpoint = your loopback model
```

## 2. Run the A/B (control vs rtmx treatment vs the Sonnet-4 baseline)

intent-bench is a bash A/B harness; `treatments/rtmx.sh` already seeds the rtmx
intent layer, so the intent A/B is built in. From the intent-bench checkout:

```bash
# local aegis — control (no intent layer)
bash bench.sh run url-shortener --agent aegis --model <local-id> --condition control --runs 5

# local aegis — rtmx treatment (intent layer seeded)
bash bench.sh run url-shortener --agent aegis --model <local-id> --condition treatment --treatment rtmx --runs 5

# hosted baseline for comparison
bash bench.sh run url-shortener --agent claude-code --model claude-sonnet-4-20250514 --condition control --runs 5
```

**Intent-tool attribution (BENCH-004):** intent-bench attributes intent-tool
tokens by tool-name prefix (default `mcp__rtmx__`). OpenCode names its rtmx MCP
tools differently, so set `INTENT_TOOL_PREFIX` to match before a treatment run;
**control** runs must use a config with rtmx MCP *off* (so `tool_tokens` and
`tool_calls_intent` are zero — the ledger rule).

## 3. Read the results

`results/summary.csv` (26-col ledger) holds every run. The numbers that matter:

| Metric | Meaning | aegis equivalent |
|---|---|---|
| **completion rate** | fraction of runs where the tests pass (PASS) | ≈ ACR (our north star) |
| total tokens (PASS) | token cost per passing run | TCR |
| knowledge_entropy | exploration spread (0–10) | — |
| backtrack rate | re-work churn | — |

Compare three cells: **local-aegis control** vs **local-aegis + rtmx treatment**
vs **Sonnet-4**. The treatment-vs-control delta isolates the **intent layer's**
effect; the aegis-vs-Sonnet-4 delta shows where a local-only stack lands against a
frontier hosted model. Stats: Fisher exact (completion rate), Mann-Whitney U
(tokens) — `analysis/compare.py`.

**Apples-to-apples caveat:** the baseline drives Claude Code (a different harness)
on Sonnet-4; we drive OpenCode on a local model. For a clean model-vs-harness
split, also run **OpenCode-on-Sonnet-4** as a second baseline once the adapter is
validated.
