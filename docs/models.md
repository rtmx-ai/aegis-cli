# Local model set + switching

aegis keeps **two pinned models local** so the operator can switch per task. Both are
recorded in `deploy/models/catalog.json` (the registry: `sha256` + `url` + per-model
`tuning`), verified against the upstream HuggingFace LFS digests.

| Model | Catalog id | Ollama tag | GGUF | When |
|---|---|---|---|---|
| **Gemma-4-26B-A4B** (QAT Q4_K_XL) | `gemma-4-26b-a4b` | `gemma4-qat:32k` | `gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf` (14.2 GB) | The proven capability winner (SERVE-016: the only model observed to complete real tool-call edits). A thinking model — tuning sets `think:false`. |
| **Qwen3-Coder-30B-A3B** (UD-Q4_K_XL) | `qwen3-coder-30b-a3b` | `qwen3-coder:30b` | `Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL.gguf` (17.7 GB) | The research-recommended agentic primary — purpose-built for tool use, **non-thinking by design**. |

Neither completes within an interactive budget on linux-cpu (no GPU) — see
`docs/serve-016-bakeoff.md`. The local set exists so SERVE-021 (darwin-metal) can
validate either, and so the operator can pick the right tool per task today.

**Provenance note.** The default (`MODEL_REF`) is the **US-origin** gemma; qwen3-coder is
PRC-origin (Alibaba). Section 889 does not bar it, but other authorities and contract terms
can — default to the non-PRC model for controlled work. See
[`docs/model-compliance.md`](model-compliance.md).

## Switching

Switching is config-level — the per-model **tuning auto-applies** from the catalog in
both paths (sampling/num_ctx/think), so you only change *which* model, not how it's tuned.

**Ollama spike** (`internal/opencode`, what the bake-off drives): set the run config's
`model_id`. Both tags are pulled locally.
```jsonc
{ "endpoint": "http://127.0.0.1:11434", "harness": "opencode", "model_id": "qwen3-coder:30b" }
//                                                              ... or "gemma4-qat:32k"
```
`config.TuningForModel` matches the tag to the catalog and renders the tuning (SERVE-020).

**llama.cpp production** (`aegis serve`, SERVE-017): point the calibration's `model` at the
GGUF. `config.TuningForGGUF` matches it to the catalog and carries the model's `num_ctx`
onto `llama-server --ctx-size` (robust, never the small default).
```jsonc
// deploy/llama-server/calibration.json
{ "target": "linux-cpu", "threads": 16, "batch": 512, "ngl": 0,
  "model": "/home/<you>/models/Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL.gguf", "ctx_size": 16384 }
```

**Bundle default** (the side-load pin in `deploy/models/MODEL_REF`): re-pin from a local
GGUF — this is also the switch for what the air-gap bundle ships as default.
```bash
scripts/pin-model.sh ~/models/Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL.gguf   # → MODEL_REF
```

## Acquiring / re-verifying

Side-load only — never fetched at runtime. On a connected build host, download from the
catalog `url`, then verify against the catalog `sha256` (the HuggingFace LFS oid):
```bash
sha256sum ~/models/<file>.gguf   # must equal the catalog sha256
```
`TestModelPinsConcrete` guards that MODEL_REF and both catalog models stay concretely
pinned (never PENDING).
