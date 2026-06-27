# Requirement Specification — Model corpus expansion + serving tuning

**Thread:** `SERVE-019..021` · **Phase 9 / sprint v1.0** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Spec parent:** `docs/serve-016-bakeoff.md`
**Follows from:** `REQ-SERVE-016` (the first bake-off) — which found that NO candidate
completes agentic coding within an interactive budget on linux-cpu, that the
coder-tuned candidates were untested, and that the serving config is not tuned per model.

## 1. Why

The SERVE-016 bake-off surfaced three gaps, each its own requirement here:

1. **The corpus was too narrow.** Only general/weak models were pulled + scored
   (gemma, phi4-mini, laguna). The coder-tuned MoE candidates already in
   `deploy/models/catalog.json` — `qwen2.5-coder-14b`, `qwen3-coder-30b-a3b` (agentic-
   coding-specialist) — were never run. A coder model may complete where the others fail.
2. **The serving config is untuned.** We send OpenCode's defaults to every model. The
   failures we saw are config-shaped: gemma "rambles" (a reasoning model with no thinking
   budget), phi4-mini/laguna emit prose instead of tool calls (no sampling discipline / no
   tool-call coaxing). Per-model sampling + context + thinking-mode tuning is the lever.
3. **The budget verdict is hardware-bound.** "No candidate within 240s" is a *linux-cpu,
   no-GPU* result. The selection must be re-validated on `darwin-metal` (GPU) before a
   production pin.

## 2. Requirements

### REQ-SERVE-019 — Expand the bake-off corpus with the coder candidates
**aegis shall** pull and score the coder-tuned candidates (`qwen2.5-coder-14b`,
`qwen3-coder-30b-a3b`) in the SERVE-016 bake-off, alongside the existing field, and record
the result. The bake-off corpus must cover at least the coder-specialist models, not only
general/weak ones. *Target:* `eval/bakeoff/results.json` includes the qwen coder candidates
with completion/WCR/TCR; the recommendation in `docs/serve-016-bakeoff.md` reflects them.
*Test:* `test::TestCoderCandidatesScored`. *Depends on:* `REQ-SERVE-016`.

### REQ-SERVE-020 — Per-model serving tuning for reliable tool-calling
**The rendered serving config shall** apply per-model tuning so candidates emit real tool
calls instead of prose or unbounded reasoning. The catalog records, per model, the
recommended knobs; the launch applies them through OpenCode's config (agent
`temperature`/`top_p`/`options`, provider model options). The knobs (final values set by the
`docs/serve-016-bakeoff.md` characterization + research):

- **Sampling**: low/zero `temperature`, recommended `top_p`/`top_k`/`min_p` per model class
  (e.g. coder models want near-deterministic sampling for tool-calling).
- **Thinking budget**: disable or bound "thinking"/"reasoning" for reasoning models
  (e.g. Qwen3 `enable_thinking=false` / `/no_think`) so they emit tool calls promptly.
- **Context**: a CPU-appropriate `num_ctx` (a 32k context is a heavy prefill cost on CPU).

*Target:* given a catalog model with recommended params, `RenderConfig` emits them so the
launched model uses them; a tuned candidate's tool-call rate improves measurably in the
bake-off. *Test:* `internal/opencode::TestPerModelTuning`. *Depends on:* `REQ-SERVE-016`.

*Delivery (implemented 2026-06-27):* the catalog gains an `ollama` tag + `tuning` per model;
`aegis run` matches the operator's model id to the catalog (`config.TuningForModel`) and
`RenderConfig` emits `agent.build.{temperature,top_p}` + `options{top_k,min_p,repeat_penalty,
num_ctx,think}`. **Sampling (temperature/top_p) is delivered reliably** through the harness;
the Ollama extensions ride `options` **best-effort** — Ollama's OpenAI-compatible endpoint may
ignore `num_ctx`/`think`, so the *robust* path for those is the serving launch (llama.cpp
`--ctx-size` per `REQ-SERVE-017`, or an Ollama Modelfile). On linux-cpu the efficacy is masked
by the prefill wall (`SERVE-021` validates on GPU).

### REQ-SERVE-021 — Re-validate the bake-off on darwin-metal (GPU)
**aegis shall** re-run the bake-off on the `darwin-metal` target and record completion + WCR
at GPU speed, so the model selection is validated against the production-latency host (the
SERVE-016 verdict was linux-cpu/no-GPU). *Target:* a `darwin-metal` bake-off run recorded
(`eval/bakeoff/results-darwin-metal.json`); the winner meets an interactive budget there or
the gap is documented. *Test:* `test::TestBakeoffDarwinMetal` (gated on a darwin-metal host;
skips on linux). *Depends on:* `REQ-SERVE-016`.

## 3. Notes

- SERVE-019 is runnable now (connected build host pulls the GGUFs); SERVE-021 needs the MBP.
- SERVE-020's specific parameter values come from the `docs/serve-016-bakeoff.md`
  characterization (how practitioners run these models) — kept out of the requirement text
  so the requirement is "apply per-model tuning", not a frozen parameter table.
- These do not re-open SERVE-016 (its bake-off ran + recorded a conditional winner); they
  broaden the field, tune the serving, and re-validate on the real hardware.

## 4. Exit criteria

SERVE-019 COMPLETE with the qwen coders scored + the recommendation updated; SERVE-020
COMPLETE with per-model tuning rendered + a measurable tool-call improvement on a tuned
candidate; SERVE-021 COMPLETE (or recorded as hardware-pending) with a darwin-metal run.
