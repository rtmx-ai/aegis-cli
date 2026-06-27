# SERVE-016 — Model bake-off

**Requirement:** REQ-SERVE-016 — *select the local model that best completes agentic
coding tasks within a latency budget.*
**Run:** `scripts/serve-bakeoff.py` · **Result:** `eval/bakeoff/results.json` ·
**Test:** `test::TestBakeoffRecorded`
**Host:** linux-cpu (Ryzen, **no usable GPU** at run time) · Ollama loopback ·
**Date:** 2026-06-27

## Method

`scripts/serve-bakeoff.py` drives the real air-gapped run (`aegis run` → `opencode
serve` → the candidate model) over a fixed set of small, deterministic Go edit tasks
(`go-add`, `go-max`: a stub that fails until the agent makes the right one-line edit),
scoring each candidate on:

- **completion** — did the agent edit the file so `go test` passes? (north star)
- **WCR** — wall-clock per task (the latency budget)
- **TCR** — tokens per task (input+output, from the intent-bench transcript)

Budget: **240s/task** (a reasonable interactive ceiling). Re-runnable with `--models`,
`--runs N`, `--timeout`.

## Candidates + results (240s budget)

| Model | Size | Completion | WCR | TCR | Behaviour |
|---|---|---|---|---|---|
| phi4-mini:latest | 2.5 GB | **0/2** | 69 s | 4457 | Fast, finishes turns, but emits no correct edit — too weak for tool calls |
| gemma4:e4b | 9.6 GB | **0/2** | 240 s (timeout) | 0 | Times out before completing |
| **gemma4-qat:32k** (gemma-4-26B-A4B-it-qat) | 15.4 GB | **0/2** | 240 s (timeout) | 0 | Times out at 240s; **does** complete given ~5–8 min (see below) |
| laguna-xs.2:latest | 23.1 GB | **0/2** | 146 s | 4096 | Finishes turns fast but emits no correct edit |

**No candidate completed any task within the 240s interactive budget on this CPU host.**

## Decision — winner: `gemma4-qat:32k` (= gemma-4-26B-A4B-it-qat), conditional

`gemma4-qat:32k` is the **capability winner**: it is the *only* candidate demonstrated to
complete real tool-call edit tasks end-to-end — verified this session in
`REQ-BENCH-007`/`REQ-BENCH-008` (`calc mul→a*b`, `Go Add`, `sub`) at ~5–8 min per turn.
The small/fast models (phi4-mini, laguna-xs.2, gemma4:e4b) finish quickly but never emit a
correct edit (the "small model too weak for tool calls" failure the SERVE spec predicted).

The selection is **conditional**, for two reasons surfaced by the bake-off:

1. **linux-cpu (no GPU) does not meet an interactive latency budget.** Every candidate
   either fails fast or times out at 240s. The capable model is CPU-bound-slow (and
   inconsistent — one `go-add` run rambled >12 min without completing). This confirms the
   project premise: **validate on linux-cpu, run production on darwin-metal (GPU)**. The
   pin must be re-validated there.
2. **The coder-tuned catalog candidates were not tested.** `qwen3-coder-30b-a3b`,
   `qwen3.6-35b-a3b`, and `qwen2.5-coder-14b` (in `deploy/models/catalog.json`) were not
   pulled into Ollama, so they are untested here. A coder-tuned model could change the
   ranking — a follow-up bake-off should pull and score them, especially on darwin-metal.

`deploy/models/MODEL_REF` already names the gemma-4-26B-A4B QAT GGUF; the bake-off
**confirms** that choice over the tested alternatives. Its `sha256` stays `PENDING` — staging
(`stage-ripgrep.sh`-style side-load) and a darwin-metal re-validation are the remaining gates
before pinning a production digest.

## Reproduce / extend

```bash
make build && ollama serve   # ensure candidates are pulled
scripts/serve-bakeoff.py --models gemma4-qat:32k,qwen3-coder-30b-a3b,laguna-xs.2:latest \
                         --runs 3 --timeout 480
```
