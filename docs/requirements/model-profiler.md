# Requirement Specification — background model profiler (PROFILE-001..)

The fast-start guarantee (OC-023) gets the operator coding immediately on whatever model is present.
The **background profiler** answers the harder question *behind* the operator: **which models actually
fit this host best** — so it can recommend an upgrade without ever blocking the launch.

## 1. What "fits" means (the model)

A model "fits" a host only if it clears **three gates**, not just RAM capacity:

1. **Capacity (necessary).**
   `required = weights + KV_cache(ctx) + compute_buffers + overhead  ≤  available_memory − reserve(OS + siblings)`
   - **weights** = the GGUF byte size (exact, from the catalog).
   - **KV cache** scales with context length: `≈ ctx × kv_bytes_per_token`, where
     `kv_bytes_per_token ≈ 2 (K+V) × n_layers × n_kv_heads × head_dim × kv_quant_bytes`. At long ctx it
     can rival the weights. Iteration 1 estimates it from model scale (params/layers heuristic);
     iteration 2 reads the exact arch from the GGUF header.
   - **available_memory** is measured *now* (`/proc/meminfo` MemAvailable on linux), not total — it
     accounts for the OS + sibling workloads (the OpenCode harness, rtmx, the verify phase). A
     reserve margin is held on top.

2. **Throughput (the gate that actually decides usability).**
   Decode is **memory-bandwidth-bound**:
   `predicted_tok_per_sec ≈ effective_bandwidth ÷ bytes_read_per_token`, where
   `bytes_read_per_token ≈ active_params × bytes_per_param` (MoE reads only active params; dense reads
   all). The host's **memory bandwidth is probed** (a STREAM-style read sweep), because the same model
   that "fits in RAM" is interactive on a 600 GB/s unified-memory Mac and a slideshow on a 50 GB/s DDR4
   Ryzen. Prefill/TTFT is compute-bound and tracked separately.

3. **Headroom (siblings).** The host is a process group, not a dedicated server. The reserve in gate 1
   and the bandwidth derate in gate 2 leave room for the OS + co-located workers; the loop already
   phase-separates generate vs verify so two bandwidth-heavy stages don't collide.

**Two floors, by mode.** "Acceptable tok/s" differs: an **interactive** floor (the TUI; a higher bar)
and an **unattended** floor (`aegis run`/loop; a lower bar, so a bigger/smarter model "fits" headless
that you'd never tolerate interactively). A model is recommended per-mode against its floor.

**Predict, then measure.** The pre-download estimate (capacity + roofline) narrows the field cheaply;
the authoritative number is empirical — iteration 2 micro-benches the top candidate's real tok/s and
steps down if it misses the floor.

## 2. Requirements

### PROFILE-001 — Host probe + resource-fit recommendation
**`aegis profile` shall** probe the host (available RAM, **memory bandwidth**, physical cores,
accelerator/target) and, for every **origin-allowed** (US-only by policy) catalog model, compute the
capacity + roofline-throughput fit and emit a **ranked recommendation** for the interactive and
unattended floors — the largest model that clears each. It writes the result to
`~/.config/aegis/profile.json` (cached) and prints a human summary. It is **read-only + non-blocking**:
it never downloads, never serves, never edits the running calibration. **Test:** `internal/profile`
unit tests (fit math + ranking) + `cmd/aegis` profile-command test.

### PROFILE-002 — Micro-bench confirmation (authoritative tok/s)
**`aegis profile --bench` shall** replace the *predicted* throughput of the model currently serving
with a *measured* one: it warms the model, times a short generation, and computes the real decode rate
(completion tokens ÷ wall-clock). It folds that authoritative figure back into the recommendation —
marking the row measured and **re-picking the floors, so a model that benches below its floor steps
down** to the next-best. With no model serving it guides the operator and falls back to prediction.
**Test:** `internal/profile` (MeasureTokPerSec + ApplyMeasurement fold/step-down).

### PROFILE-003 — First-launch auto-profile + gentle fit hint
**Bare `aegis` shall** profile the host once on first launch (after the model is up, ~0.3 s, cached to
`~/.config/aegis/profile.json`; a no-op on re-launch) and surface a one-line, non-intrusive hint at
launch naming the best-fitting US model — with an upgrade nudge when the running model isn't it. It
never blocks the TUI. **Test:** `cmd/aegis` (profile hint + cache round-trip + auto-profile).

## 3. Scope

**Iteration 1 (this build):** the probe (linux available-RAM + bandwidth sweep + reuse
`internal/install.Detect`), the fit model (capacity + roofline; params/active derived from catalog
size + id, KV by scale heuristic), the ranking, `aegis profile`, the cached `profile.json`.

**Iteration 2:** `--bench` micro-bench confirmation (PROFILE-002) — authoritative tok/s for the
running model, folded into the recommendation with floor step-down.

**Iteration 3 (this build):** first-launch auto-profile + the launch fit hint (PROFILE-003).

**Deferred (noted, not built):** exact KV from GGUF header parsing; darwin available-RAM + Metal
bandwidth. The recommendation is **advisory** — the operator chooses; `pin-model.sh` + `bench.sh`
remain the provisioning path.

## 4. Acceptance

- `aegis profile` on linux-cpu prints a bandwidth figure > 0 and a per-model table (fits / predicted
  tok/s / capacity) for the US models, with an interactive + unattended pick.
- No egress, no serve, no calibration mutation (it's a pure read of host + catalog).
- The fit math is unit-tested deterministically (capacity + throughput against known inputs); the
  probe has a smoke test (bandwidth > 0, available ≤ total).
