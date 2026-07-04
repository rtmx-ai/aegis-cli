# Local model set + selection

aegis ships a **curated catalog** of local models (`deploy/models/catalog.json` — the
registry: `sha256` + `url` + per-model `tuning`, verified against the upstream HuggingFace
LFS digests) and lets **the host choose**, because the binding constraint is memory
(capacity + bandwidth), not preference.

| Model | Catalog id | Origin | Arch | Notes |
|---|---|---|---|---|
| **Gemma-4-26B-A4B** (QAT Q4_K_XL) | `gemma-4-26b-a4b` | US (Google) | MoE, ~4B active | **Bundle default** (`MODEL_REF`). Provenance-safe for controlled/ITAR work; the low active-param count makes it fast on memory-bandwidth-bound hosts. Thinking model — tuning sets `think:false`. |
| **Devstral-Small-2507** (IQ4_XS) | `devstral-small-2507` | FR (Mistral) | dense 24B | **Non-PRC agentic coder** (Apache-2.0), purpose-built for tool-calling. Stronger in the abstract, but dense → ~6× the bytes/token of the MoE, so it needs a host with the bandwidth to feed it. |
| **Qwen3-Coder-30B-A3B** (UD-Q4_K_XL) | `qwen3-coder-30b-a3b` | CN (Alibaba) | MoE, ~3B active | Coding-specialist MoE. PRC-origin — denied by default policy; opt in explicitly if permitted for your work. |

…plus others (Laguna-XS.2, Phi-4-mini). See the full set in the catalog.

## Selection is measured, not assumed

The host decides which model to run, in two steps:

- **`aegis profile`** — for every catalog model that clears the origin policy, predicts
  whether it fits this host's memory and its decode throughput (tok/s), and recommends the
  largest that clears an interactive/unattended floor. (On a 24 GB / ~59 GB/s M5 Pro, for
  example, the ~4B-active Gemma runs ~16 tok/s while dense 24B Devstral is ~3 tok/s.)
- **`aegis bakeoff`** — *measures* the host-suitable models head-to-head on a fixed suite
  of real coding tasks: **files-edited** (did it actually write code), **ACR** (did the
  edit pass the test), and real decode **tok/s**. Run bare to pick interactively, or
  `--all` to download + serve + measure every suitable model. It serves each candidate
  itself and records the served model, so the comparison is valid (a same-model run is
  refused, not silently ranked). See `docs/serve-016-bakeoff.md`.

aegis also **sizes the served context to the host** at launch (weights + KV must fit
available memory): a 24 GB box lands at ~16k, a roomy box at 32k — no manual tuning.

**Provenance note.** The default (`MODEL_REF`) is **gemma-4-26b-a4b** (US-origin, Google) —
the provenance-safe pick for controlled/ITAR work, chosen so a controlled deployment is
correct by default. `deploy/models/origin-policy.json` is **default-deny**: US is allowed,
FR is an explicit, auditable opt-in for the non-PRC Mistral coder, and PRC origins (qwen)
stay denied unless similarly opted in. Section 889 does not bar qwen, but other authorities
and contract terms can. Re-pin the default with `scripts/pin-model.sh`. See
[`docs/model-compliance.md`](model-compliance.md).

## Switching

Switching is config-level — the per-model **tuning auto-applies** from the catalog in
both paths (sampling/num_ctx/think), so you only change *which* model, not how it's tuned.

**Default when a run names no model:** `aegis run` resolves `gemma4-qat:32k` on both
targets (`DefaultModelForTarget`) — the provenance-safe, memory-bandwidth-friendly default
that closes real tasks on CPU (RUNQ-004) and runs fast on Metal. Name a model explicitly to
override, or let `aegis profile` / `aegis bakeoff` recommend the best fit for the host.

**Ollama spike** (`internal/opencode`, what the bake-off drives): set the run config's
`model_id`. Both tags are pulled locally.
```jsonc
{ "endpoint": "http://127.0.0.1:11434", "harness": "opencode", "model_id": "qwen3-coder:30b" }
//                                                              ... or "gemma4-qat:32k"
```
`config.TuningForModel` matches the tag to the catalog and renders the tuning (SERVE-020).

**llama.cpp production** (`aegis serve`, SERVE-017): point the calibration's `model` at the
GGUF. The context is **not** pinned in the calibration — aegis resolves it at launch (env
`AEGIS_CTX_SIZE` > catalog `num_ctx` > default) and **caps it to fit the host's memory**
for that model, so a big model on a small box auto-sizes instead of overflowing.
```jsonc
// deploy/llama-server/calibration.json — no ctx_size; aegis sizes it to the host at launch
{ "target": "linux-cpu", "threads": 16, "batch": 512, "ngl": 0,
  "model": "/home/<you>/models/gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf" }
```

**Bundle default** (the side-load pin in `deploy/models/MODEL_REF`): re-pin from a local
GGUF — this is also the switch for what the air-gap bundle ships as default.
```bash
scripts/pin-model.sh ~/models/gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf   # → MODEL_REF (the default)
```

## Acquiring / re-verifying

Side-load only — never fetched at runtime. On a connected build host, download from the
catalog `url`, then verify against the catalog `sha256` (the HuggingFace LFS oid):
```bash
sha256sum ~/models/<file>.gguf   # must equal the catalog sha256
```
`TestModelPinsConcrete` guards that MODEL_REF and both catalog models stay concretely
pinned (never PENDING).
