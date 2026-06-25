# Requirement Specification — Full-Stack Build Cadence + Model Staging

**Threads:** MODEL (staging), BUILD-010..012 (tiered cadence) · **Phase 9 / v1.0**
**Tracked in:** `.rtmx/database.csv` · **Skills:** `airgap-hygiene`, `build-to-spec`

## 0. Why

aegis is a *full stack*: the Go orchestrator **+ OpenCode** (built from pinned
source) **+ llama.cpp** (built from pinned source) **+ a model GGUF**. Two honesty
gaps surfaced:
1. We pin + build OpenCode (`OPENCODE_REF`) and llama.cpp (`LLAMA_REF`), but the
   **model GGUF has no pin / stage / verify** — the missing third leg.
2. The per-commit/per-push gate (`make ci`) builds + checks **only the Go binary**;
   it does not build the rest of the stack, and nothing integration-tests the
   stack together.

The fix is **not** "build everything on every commit" (too heavy) — it's an
explicit **tiered cadence**.

## 1. Tiered cadence (the contract)

| Tier | Trigger | What runs | Cost |
|---|---|---|---|
| **fast gate** | pre-commit | `make ci-fast` (fmt/lint/vet/build/test/health) | seconds |
| **full gate** | pre-push, CI push/PR | `make ci` (+ race/cover/vuln/airgap/metrics) | ~1–2 min |
| **full stack** | `make ci-full` locally; release/tag/nightly in CI | build OpenCode + llama.cpp from pinned source + **stage model** + **integration smoke** | minutes |

The fast/full gates stay **Go-only** (correct — the stack build is too heavy per
commit). The **full-stack** tier is where "build the whole thing" lives, with
`make ci-full` giving developers **local parity** with the release/nightly tier
(same single-source-of-truth philosophy as `make ci`, extended to the stack).

## 2. Requirements

### MODEL — the third leg (pin + stage + verify the GGUF)
- **REQ-MODEL-001** — pin the selected model GGUF by **name + sha256** in
  `deploy/models/MODEL_REF`; the digest feeds the SERVE-002 served-model gate. The
  concrete model is the SERVE-016 bake-off winner (placeholder until then).
- **REQ-MODEL-002** — `scripts/stage-model.sh` acquires + **verifies** the pinned
  GGUF on the connected host (copy/fetch → sha256 check → stage), **refusing on
  digest mismatch**. Mirrors `build-opencode.sh` / `build-llama.sh`.

### BUILD — tiered full-stack cadence
- **REQ-BUILD-010** — `make ci-full` = `make ci` + `build-opencode.sh` +
  `build-llama.sh` (+ `stage-model.sh` if a pin is present): one command builds the
  entire stack locally, matching the release/nightly tier.
- **REQ-BUILD-011** — the **release tier** builds the full stack (OpenCode via
  OC-007 + llama-server) from pinned source; the per-push gate stays Go-only.
- **REQ-BUILD-012** — a **full-stack integration smoke**
  (`scripts/integration-smoke.sh`, gated): bring the stack up on loopback, run
  `aegis run` on a tiny task, assert EGRESS=0. Distinct from the closed-host
  ENCLAVE-003 manual smoke.

## 3. Local-CI parity (why not `act`/`gh`)

Parity is the **Makefile as single source of truth**: `make ci` is invoked
*identically* by the pre-push hook and CI. `make ci-full` extends that to the
stack. We deliberately do **not** use `act` (Docker-heavy; the org restricts
third-party actions, so workflows are thin) — the Makefile guarantees gate parity
without it. The residual blind spot is the workflow YAML itself (matrix/jobs/
actions), accepted as a known trade-off.

## 4. Exit criteria

MODEL-001/002 + BUILD-010/011 COMPLETE via `rtmx verify` (inspection); BUILD-012
gated on a staged model (runs at the release tier / on the validation host).
`make ci-full` builds aegis + OpenCode + llama-server from pinned source on a
toolchain-equipped host.
