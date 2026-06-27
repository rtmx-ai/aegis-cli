# Running aegis against intent-bench

[intent-bench](https://github.com/intent-bench/intent-bench) measures whether structured
intent improves a coding agent's effectiveness on **multi-requirement greenfield project
builds** (14 experiments — url-shortener, task-manager, rest-api, …). rtmx is a first-class
**treatment** there (`treatments/rtmx.sh`); aegis plugs in as the **agent** that runs a
local, air-gapped model. This dir holds the aegis agent wrapper so the comparison is real
(not the toy demo in `scripts/intent-bench.py` / `docs/intent-bench.md`).

## Setup

```bash
# Check intent-bench out as a peer of aegis-cli
git clone https://github.com/intent-bench/intent-bench.git \
    ~/code/github.com/intent-bench/intent-bench
cd ~/code/github.com/intent-bench/intent-bench
make setup && make validate

# Install the aegis agent wrapper + ensure aegis + rtmx are reachable
cp <aegis-cli>/deploy/intent-bench/aegis.sh agents/aegis.sh && chmod +x agents/aegis.sh
export AEGIS_BIN=<aegis-cli>/bin/aegis     # aegis binary
ollama serve                                # or bring up llama-server via `aegis serve`
```

## Run

```bash
# CONTROL (no intent) and TREATMENT (rtmx) for one experiment on the local model
AEGIS_TIMEOUT=1800s bash bench.sh run url-shortener --condition control   --agent aegis --model gemma4-qat:32k --runs 5
AEGIS_TIMEOUT=1800s bash bench.sh run url-shortener --condition treatment --agent aegis --model gemma4-qat:32k --runs 5 --treatment rtmx
make analyze   # Mann-Whitney U (tokens) + Fisher exact (completion) -> results/analysis.json
```

The wrapper runs `aegis run --no-intent`, so intent-bench controls the A/B via the workdir:
its `treatments/rtmx.sh` seeds the experiment's requirements as an rtmx MCP (`.mcp.json`) for
**treatment**, and leaves it absent for **control**. aegis does not inject its own intent
layer, keeping the comparison clean. Endpoint via `AEGIS_ENDPOINT` (default Ollama loopback).

## Reality check (read before a full run)

- **Scale.** Each experiment is a *complete project build* (e.g. a working URL shortener with
  storage + tests). intent-bench's default agent is cloud Sonnet-4 (~$5/experiment). On a
  **local CPU model** each session is tens of minutes, and a full benchmark (14 exp × 2
  conditions × N≥5) is days–weeks of CPU. This wants **GPU** (`SERVE-021`) and/or a reduced
  experiment set.
- **Capability.** A small local model (gemma/qwen3-coder) may not complete full greenfield
  builds at all — a low/zero completion rate is a *real result*, not a bug. The point is to
  measure it honestly, control vs treatment, on the same footing.

## Submitting results (intent-bench's process)

Per intent-bench `REPRODUCING.md`: fork intent-bench, run with the documented commands,
**append** to `results/summary.csv` (never overwrite), commit `summary.csv` + `analysis.json`,
and open a PR titled `results: aegis <model> <experiment> N=<n>` with model id, date,
N/condition, hardware/OS, and any deviations. This satisfies `REQ-BENCH-009`.
