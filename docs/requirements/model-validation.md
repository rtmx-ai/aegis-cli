# Requirement Specification — Serving Readiness & Real-Model Validation

**Thread:** `SERVE-012..014`, `DOCS-004` · **Phase 4–5 / sprint v0.4–v0.5** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `serving-calibration`, `metrics-eval`, `airgap-hygiene`, `rtmx-loop`

## 1. Purpose & scope

aegis-cli can already call a local model over loopback: `internal/serving/client.go`
round-trips chat completions (REQ-SERVE-006), streams (REQ-SERVE-007), reads the
served model id+digest (`ModelInfo`, REQ-SERVE-010), and probes `/health`
(`Health`, REQ-SERVE-001). What is missing is the **readiness gate** in front of
a run and the **profiling feed** behind it: nothing confirms the *right* model is
actually serving and answering before work is claimed, nothing surfaces per-call
timing/tokens into the metrics dashboard, and nothing documents how to validate
the stack against a **real GGUF** — which is intrinsically not CI-testable
because no model ships in CI.

This thread closes those serving-readiness gaps and writes the real-model
validation procedure. After it: a run fails fast if the model is not serving
(preflight smoke), aborts if a weak/wrong/quant-mismatched model is loaded
(digest gate), feeds the prefill/decode timing breakdown and WCR/TCR with real
per-call numbers, and the enclave operator has a rigorous, repeatable runbook for
validating the actual model.

In scope: a minimal preflight chat request with a timeout; a run-start model
id+digest gate against configured expectations; per-call latency + prompt/
completion token reporting consumable by `internal/metrics`; and the operator
runbook documenting calibrate → smoke → golden-set ACR on the real model. Out of
scope: the model bake-off itself, calibration sweep mechanics (REQ-SERVE-003 /
`scripts/bench.sh`), the loop's preflight wiring in `aegis run` (REQ-RUN-002,
`docs/requirements/live-run.md`), and any non-loopback network path (forbidden by
construction).

## 2. Definitions

- **Preflight smoke** — a single minimal chat completion issued against the
  endpoint, under a bounded timeout, before any requirement is claimed. It proves
  the model is not merely reachable (`/health`) but actually *generating*.
- **Digest gate** — a run-start comparison of the served model's id+digest
  (`ModelInfo`, REQ-SERVE-010) against the configured expected value. A mismatch
  aborts; this extends REQ-SERVE-002's quant/digest idea from launch to run start.
- **Call timing** — per-`ChatCompletion` wall-clock latency plus the prompt and
  completion token counts, structured so `internal/metrics` can fold them into
  WCR/TCR and the prefill/decode breakdown (CLAUDE.md §5).
- **Real-model validation** — the manual, out-of-CI procedure that exercises the
  full serving stack against the actual GGUF on the enclave host, because no model
  is available in CI. Verified by inspection, not by `rtmx verify`.

## 3. Requirements

Each requirement is well-formed (EARS style), independently verifiable, and
linked to the acceptance test that closes it via `rtmx verify`. SERVE-012/013/014
are mock-backed (`internal/mockmodel`) and so are CI-closable; DOCS-004 is an
Inspection requirement closed by a doc-presence test.

### REQ-SERVE-012 — Preflight smoke completes before a run
**Before a run begins, the serving client shall** complete a minimal chat
completion against the endpoint within a bounded timeout, and **when** the
endpoint is dead or unresponsive **it shall** return a timely, typed error rather
than hang.
*Rationale:* fail fast — `/health` only proves reachability; a smoke completion
proves the model is actually serving before any requirement is claimed.
*Acceptance:* a minimal completion succeeds against the endpoint (mock in CI)
within the timeout; a dead endpoint yields a timely typed error and does not block
indefinitely. *Test:* `internal/serving::TestPreflightSmoke`.
*Depends on:* REQ-SERVE-006.

### REQ-SERVE-013 — Run-start model digest gate
**At run start the client shall** check the served model id+digest
(`ModelInfo`) against the configured expected value; **when** they mismatch **it
shall** abort with a clear error, and **when** they match **it shall** proceed.
*Rationale:* a weak, wrong, or quant-mismatched model silently degrades ACR; the
digest gate is the control. It extends REQ-SERVE-002's launch-time check to run
start, where the served weights are observable.
*Acceptance:* a mismatch aborts with a clear, typed error naming expected vs.
served; a match proceeds without error. *Test:*
`internal/serving::TestModelDigestGate`. *Depends on:* REQ-SERVE-010.

### REQ-SERVE-014 — Per-call timing + token counts for the dashboard
**The client shall** surface, for each `ChatCompletion`, the call's wall-clock
latency and its prompt/completion token counts in a form consumable by
`internal/metrics`.
*Rationale:* feeds WCR/TCR and the prefill/decode timing breakdown (CLAUDE.md §5)
with real per-call numbers rather than estimates — the metrics *are* the profiler.
*Acceptance:* `ChatCompletion` reports latency and prompt/completion token counts
that `internal/metrics` can fold into an `Attempt` (Tokens, WallClock, Stages).
*Test:* `internal/serving::TestClientReportsTiming`. *Depends on:* REQ-SERVE-006.

### REQ-DOCS-004 — Real-model validation runbook
**A real-model validation runbook shall** document, as a manual enclave-host
procedure, the sequence calibrate (`scripts/bench.sh`) → preflight smoke →
digest verification → golden-set ACR on the actual GGUF, with explicit pass
thresholds.
*Rationale:* the real-model step cannot run in CI (no model ships there); it must
be a rigorous, repeatable manual procedure the operator can follow on the enclave
host, cross-referenced from `docs/runbook.md` and `docs/airgap-setup.md`.
*Acceptance:* the runbook exists (a new `docs/model-validation.md`) and covers
calibration, preflight smoke, digest verification, and golden-set ACR acceptance
with thresholds. *Test (Inspection):* `test::TestModelValidationRunbookPresent`
(asserts the doc exists and covers those topics). *Depends on:* REQ-SERVE-012.

## 4. Design constraints

- **Std-lib only** in the shipped client path (enforced by
  `test.TestRuntimeBinaryIsStdLibOnly`); the preflight, gate, and timing all build
  on `internal/serving/client.go` as it stands today.
- **Loopback-only.** Preflight, the digest gate, and timing all run over the
  existing `Client`, which refuses a non-loopback endpoint at construction; no new
  network surface is introduced.
- **Digest gate compares against config.** The expected id+digest is a configured
  value (extending the SERVE-002 quant/digest idea); the gate reads the served
  value via `ModelInfo` (REQ-SERVE-010) and compares — it does not infer
  correctness from `/health` alone.
- **Timing feeds, not stores.** REQ-SERVE-014 only *surfaces* latency + token
  counts; aggregation, WCR/TCR, and the prefill/decode breakdown stay in
  `internal/metrics`. The client adds no metrics state of its own.
- **CI validates with the mock only.** SERVE-012/013/014 close against
  `internal/mockmodel` (loopback `httptest`, programmable id/digest/responses, no
  GGUF). The **real-model** validation is explicitly out of CI: it is an operator
  runbook closed by Inspection (REQ-DOCS-004), not by a CI test run.
- **DOCS-004 ships a new doc.** Its implementation is a new operator file,
  `docs/model-validation.md`, in the same tone as `docs/runbook.md` /
  `docs/airgap-setup.md` and cross-referenced from both. The Inspection test
  asserts the file's presence and topic coverage.

## 5. Verification & exit criteria

The thread is complete when all four requirements are `COMPLETE` via
`rtmx verify --update`: SERVE-012, SERVE-013, and SERVE-014 close mock-backed
through `internal/mockmodel` (preflight smoke, digest gate, and per-call timing
respectively), and DOCS-004 closes by Inspection via
`test::TestModelValidationRunbookPresent`. `rtmx health` must be HEALTHY at 100%
coverage (no orphaned requirements or tests), the serving endpoint health must
report HEALTHY, and `make ci` must be green (race, lint, govulncheck, cover-gate
≥ floor, EGRESS=0, TRACE=100%, ACR-regression). The real-GGUF validation itself
is run manually per `docs/model-validation.md` on the enclave host and is **not**
a CI gate. Build order follows the dependency graph: SERVE-012 and SERVE-014
(on REQ-SERVE-006) and SERVE-013 (on REQ-SERVE-010) first, then DOCS-004
(on SERVE-012).

## REQ-SERVE-015 — Digest gate checks membership, not the first model

**The model id/digest gate shall** verify the expected model is **among** the
served models (`/v1/models`), not that it equals the first-listed model.

*Rationale:* surfaced by real-model validation against Ollama. `CheckModel`
compared `data[0]`, which is "the model" only on a single-model backend
(llama-server). On a multi-model backend (Ollama) `data[0]` is arbitrary, so a
correctly-served model failed the gate. Membership is correct for both: a
single-model list has one entry; a multi-model list is searched.

*Acceptance:* `CheckModel(id, digest)` passes when some served model matches the
expected id (and digest, if given) and errors when none do. *Test:*
`internal/serving::TestCheckModelMultiModel`. *Depends on:* REQ-SERVE-013.
