# intent-bench — aegis control vs rtmx-treatment vs cloud baseline

**Requirement:** REQ-BENCH-009 (+P01 runner, +P02 real run) · **Runner:**
`scripts/intent-bench.py` · **Data:** `eval/intent-bench/{summary.csv,comparison.json}` ·
**Test:** `test::TestIntentBenchSuiteRun` · **Run:** 2026-06-27, linux-cpu (no GPU).

## Method

The runner drives **every** experiment through three conditions and scores each by
`go test` (closed = the agent edited the file so tests pass):

- **control** — `aegis run --no-intent`: the local model with **no** rtmx intent layer.
- **treatment** — `aegis run`: the local model **with** the rtmx MCP intent layer.
- **baseline** — claude-code (cloud, Sonnet-class): the out-of-enclave reference. This is
  the **only** condition that leaves the host — egress is inherent to a cloud baseline.

Experiments: `go-add`, `go-max`, `go-fib` (small, real, deterministically scorable Go
tasks). Local model: `gemma4-qat:32k` (Ollama loopback) — the CPU-reliable completer.

## Result (1 run/cell)

| Condition | Completion | Tokens (total) | ~Wall/task |
|---|---|---|---|
| control (local, no rtmx) | **3/3** | 91,005 | ~260–450s |
| treatment (local, rtmx MCP) | **3/3** | 104,148 | ~270s |
| baseline (cloud claude-code) | **3/3** | 8,413 | ~9s |

control-vs-treatment Fisher exact **p = 1.0**.

## What it says (honestly)

1. **On self-contained tasks, the rtmx intent layer is completion-neutral** — control and
   treatment both close 3/3 (p = 1.0). The intent layer adds **~14% tokens** (104k vs 91k:
   the rtmx MCP tool definitions in context) with no completion change here. That is the
   *expected* result for tasks that need no scoping or verification — the intent loop's
   value (requirement framing, verify-driven retry, backlog drain) shows on **ambiguous /
   multi-step** work, which this 3-task suite does not exercise. Demonstrating that lift
   needs a larger, harder experiment set (future work).
2. **The local model matches the cloud baseline on completion** for these tasks (3/3 vs 3/3)
   — but at a steep **local-model tax**: ~30× the wall-clock (~270s vs ~9s) and ~12× the
   tokens per task. On CPU, that tax is latency you trade for the air-gap. GPU (`SERVE-021`)
   is what narrows it.
3. **1 run/cell** is a demonstration, not a tight rate. gemma is deterministic enough here;
   a model with stochastic tool-calling (qwen3-coder, see `docs/serve-016-bakeoff.md`) needs
   multiple runs per cell. The runner takes `--runs N`.

## Reproduce / extend

```bash
make build && ollama serve   # gemma4-qat:32k pulled
scripts/intent-bench.py --model gemma4-qat:32k --conditions control,treatment,baseline --runs 1
# air-gapped only (no cloud baseline):
scripts/intent-bench.py --conditions control,treatment
```

Add experiments to `EXPERIMENTS` in `scripts/intent-bench.py`; the runner enumerates them
all per condition. The cloud baseline auto-skips when `claude` is not on PATH.
