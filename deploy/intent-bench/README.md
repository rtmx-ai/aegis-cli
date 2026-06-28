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

For `make analyze` (scipy/pandas), use a venv — Ubuntu blocks system pip (PEP 668):

```bash
python3 -m venv .venv && . .venv/bin/activate && pip install -r analysis/requirements.txt
```

## Run — one model at a time, separate runs per model

`run-suite.sh` brings the model up via `aegis serve` (loopback), runs the experiments under
**control** + **rtmx treatment**, then tears the server down. Run it **once per model** —
intent-bench tags every `results/summary.csv` row by `--model`, so two models land as
distinguishable rows for `make analyze` and **separate per-model PRs**.

```bash
AB=<aegis-cli>; IB=~/code/github.com/intent-bench/intent-bench

# Model A — gemma (six "claim" experiments, N=5, ~1hr/run each on CPU):
$AB/deploy/intent-bench/run-suite.sh --bench $IB \
    --gguf ~/models/gemma-4-26B-A4B-...gguf --model-id gemma-4-26b-a4b --runs 5

# Model B — qwen3-coder (a SEPARATE run, same experiments, different port):
$AB/deploy/intent-bench/run-suite.sh --bench $IB \
    --gguf ~/models/Qwen3-Coder-30B-...gguf --model-id qwen3-coder-30b --runs 5 --port 8091

cd $IB && make analyze   # Mann-Whitney U (tokens) + Fisher exact (completion) -> results/analysis.json
```

Defaults: the six "claim" experiments (url-shortener, task-manager, rest-api, cli-tool,
brownfield, rtmx-self), N=5. Override with `--experiments "..."`, `--runs N`, `--timeout`. A
single experiment at `--runs 1` is the wiring smoke. The wrapper runs `aegis run --no-intent`,
so intent-bench controls the A/B via the workdir (rtmx `.mcp.json` seeded for treatment, absent
for control) — aegis never injects its own intent layer, keeping the comparison clean.

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
