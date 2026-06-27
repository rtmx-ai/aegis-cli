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

---

# Corpus expansion + characterization (SERVE-019, 2026-06-27)

Expanded the field with the coder specialists and a research-tuned variant. **Every
candidate still fails on linux-cpu** — the result is now decisive about *why*.

| Model | Budget | Completion | Note |
|---|---|---|---|
| qwen2.5-coder:14b | 300s | 0/2 | dense 14B — too slow on CPU |
| qwen3-coder:30b (untuned) | 300s | 0/1 | MoE 3B-active, research's top agentic pick — times out with our **default ~4k num_ctx** (truncates tool defs) |
| **qwen3-coder:tuned** (num_ctx 16384, temp 0.7/top_p 0.8/top_k 20/rep 1.05) | 480s | 0/1 | **Tuned per the research — STILL times out.** 16k prefill on CPU is too slow. |

## How others run these models (research, cited in commit) — vs our setup

The investigation (gemma/phi4/qwen-coder local-inference best practices) found our two
failure classes have distinct root causes, and that **our serving is untuned**:

| Knob | Effective setup (others) | **Our setup** | Effect of the gap |
|---|---|---|---|
| **Model for tool use** | Qwen3-Coder-30B-A3B (purpose-built agentic, *non-thinking by design*) or Qwen2.5-Coder | a general reasoning model (gemma4) + weak models (phi4-mini, laguna) | gemma rambles; phi4/laguna can't tool-call |
| **`num_ctx`** | explicitly 16k–32k (OpenCode/Cline/aider all require it) | **unset → Ollama 4k floor**, which **silently truncates oldest-first** | OpenCode's front-loaded tool defs get dropped → models emit prose instead of calls |
| **Thinking** | disable for agentic loops — Ollama top-level `"think": false` (sibling of `messages`, *not* in `options`); Qwen3-Coder needs nothing (non-thinking) | thinking left **on** for gemma4 | gemma generates huge reasoning chains → 4-min timeout |
| **Sampling** | vendor cards: Qwen `temp 0.7, top_p 0.8, top_k 20, min_p 0, rep 1.05` (explicitly **not** greedy/temp 0); Gemma `temp 1.0` | **bare defaults**, untuned per model | undisciplined sampling; no per-model values |
| **Tool-call hygiene** | `stream:false` on tool turns; precheck the Ollama template advertises the `tools` capability; expect empty `content` when a tool is called | none of these | known Ollama tool-call leak/format bugs unguarded |
| **Host** | GPU (prefill is cheap) | **linux-cpu, no GPU** | 16k prefill dominates WCR → even a tuned, correct model times out |

Sources (selected): qwenlm.github.io/blog/qwen3-coder · ollama.com/library/qwen3-coder ·
docs.ollama.com/capabilities/thinking · docs.ollama.com/context-length ·
opencode.ai/docs/providers · github.com/ollama/ollama/issues/{9437,15539,12557,8337} ·
huggingface.co/microsoft/Phi-4-mini-instruct · ai.google.dev/gemma/docs/capabilities/thinking

## Conclusion

The bake-off + characterization converge: **the limiter is the GPU-less CPU host, not the
model.** The config tuning is *necessary* (without 16k `num_ctx`, OpenCode's tool defs
truncate and even strong models emit prose; without `think:false`, gemma4 rambles), but on
CPU the required 16k prefill is too slow to finish an interactive turn. The path forward:

1. **SERVE-020** — apply per-model tuning in the rendered config: `num_ctx≥16k`,
   `think:false` for thinking models, Qwen sampling for coder models. Necessary regardless.
2. **Switch the primary candidate to `qwen3-coder:30b`** (non-thinking, agentic, MoE) — the
   research's top pick; retire phi4-mini/laguna from the tool-calling loop.
3. **SERVE-021** — re-validate on darwin-metal (GPU), where 16k prefill is fast; that is
   where the tuned qwen3-coder is expected to actually complete within an interactive budget.

---

# Fairness caveat (added 2026-06-27, SERVE-022)

**The qwen3-coder result above is confounded — do not read it as a fair capability verdict.**
The RUNQ-004 transcript showed qwen3-coder *did* call the edit tool, but in its Qwen-native
**XML** form (`<function=write>…`), which the OpenCode → Ollama **OpenAI-compatible** path
leaked into message text instead of executing — so it scored as "no edit." It also ran at
Ollama's small default `num_ctx` (gemma's tag carries 32768). Per the research, llama.cpp's
**`--jinja`** parses Qwen3-Coder's XML tool tags — so qwen3-coder may pass once served
correctly. A **fair re-comparison** (parsing fidelity verified per candidate, qwen via
`--jinja`, equal-footing context) is tracked as **REQ-SERVE-022**
(`docs/requirements/fair-model-comparison.md`). Until then, the "gemma wins / qwen fails on
CPU" finding is **harness-conditional**, and the CPU default (gemma) reflects *what parses
today*, not a settled capability ranking.
