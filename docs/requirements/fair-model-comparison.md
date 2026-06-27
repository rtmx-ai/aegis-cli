# Requirement Specification — Fair model comparison (tool-call parsing fidelity)

**Requirement:** `SERVE-022` · Status: PLANNED (future)
**Tracked in:** `.rtmx/database.csv` · **Companions:** `docs/serve-016-bakeoff.md`, `docs/readiness.md`
**Follows from:** the RUNQ-004 diagnosis — the SERVE-016/019 bake-off comparison was
**confounded**, not fair.

## 1. Why — the comparison was unfair

The SERVE-016/019 bake-off concluded that `qwen3-coder:30b` fails on CPU (times out /
fast-fails, no edit). The RUNQ-004 transcript shows that conclusion is **partly a harness
artifact, not a capability result**:

- qwen3-coder **did** call the edit tool — but in its **Qwen-native XML** format
  (`<function=write><parameter=content>…`).
- Driven through OpenCode → Ollama's **OpenAI-compatible** `/v1/chat/completions` endpoint,
  that XML was **not parsed into a structured `tool_call`** — it **leaked into the message
  text** as prose. OpenCode saw chatter, never executed the write, and the file was never
  edited → scored as "failed to complete."
- Contributing: the Ollama `qwen3-coder:30b` tag carries **no `num_ctx` override** (it runs
  at Ollama's small default context), while `gemma4-qat:32k` carries `num_ctx 32768`. SERVE-020's
  `num_ctx` tuning can't fix it over the OpenAI endpoint (the documented best-effort gap).

So gemma "won" partly because it happened to emit a tool-call format the Ollama OpenAI path
parses and ran at a context that fit the tool defs — **not necessarily because it is more
capable**. A model must be scored on **capability**, not on whether a particular serving
path happens to parse its tool-call dialect. Per the research, llama.cpp's **`--jinja`** is
mandatory to parse Qwen3-Coder's XML tool tags — so qwen3-coder may well pass on the
llama.cpp path (SERVE-017) where it failed on the Ollama spike.

## 2. Requirement

### REQ-SERVE-022 — Fair model comparison via tool-call parsing fidelity
**The model comparison shall** drive each candidate through a serving path that correctly
parses its native tool-call format, and **verify per-candidate parsing fidelity before
scoring**, so a model is ranked on capability rather than a harness artifact.

*Acceptance:*
- A **tool-call fidelity probe** per candidate confirms its tool calls arrive as structured
  `tool_calls` (executed), not leaked into message text. A candidate that mis-parses is
  recorded as **harness-incompatible**, distinct from **incapable** (failed-to-complete).
- `qwen3-coder` is driven via **llama.cpp `--jinja`** (its XML tool tags parsed), and/or an
  Ollama tag with a verified tool template + `num_ctx` override — never the bare Ollama
  OpenAI path that leaks its format.
- Every candidate is scored at **equal footing**: a tool-parsing-correct path and a context
  window that fits the harness's tool definitions (≥16k).
- The SERVE-016/019 ranking is **re-validated** under these conditions and the recorded
  result + `docs/serve-016-bakeoff.md` updated; if qwen3-coder completes once parsed
  correctly, the "qwen fails on CPU" finding is corrected.

*Test:* `test::TestFairModelComparison` (gated — reads a recorded fidelity/comparison
artifact, e.g. `eval/bakeoff/fidelity.json`). *Depends on:* `REQ-SERVE-017`, `REQ-SERVE-019`.

## 3. Implementation notes

- Add **`--jinja`** to the llama.cpp launch (`internal/serving.LaunchArgs`) — correct tool
  parsing on the production path regardless, and a precondition for the fair run.
- Add a **fidelity probe** to `scripts/serve-bakeoff.py`: issue one tool-requiring prompt and
  assert the response carries an executed tool call (transcript shows a tool result), not
  tool markup in text. Record per-candidate `parsed: true|false`.
- This is **parsing fidelity, not latency** — it can run on CPU (llama.cpp `--jinja`). It is
  complementary to `SERVE-021` (GPU re-validation for interactive latency): SERVE-022 makes
  the comparison *fair*; SERVE-021 makes it *fast*. A definitive ranking wants both.

## 4. Exit criteria

SERVE-022 COMPLETE when: each candidate is scored on a parsing-correct path with fidelity
recorded; qwen3-coder's tool calls parse (via `--jinja`); the SERVE-016/019 ranking is
re-validated fairly and the docs corrected.
