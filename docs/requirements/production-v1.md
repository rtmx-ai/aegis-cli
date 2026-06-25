# Requirement Specification — Road to a Production-Ready, Usable aegis (v1.0)

**Threads:** SERVE bake-off, RUNQ (run-hygiene), ENCLAVE (enclave-validation),
REL (release-v1) · **Phase 9 / sprint v1.0** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `serving-calibration`,
`metrics-eval`, `airgap-hygiene`, `unattended-operation`

## 0. Definition of done (v1.0)

A signed, multi-platform aegis release that, on a closed/air-gapped host: installs
from one artifact set; bare `aegis` opens a working OpenCode TUI on a local model;
`aegis run <prompt>` completes a real coding task; `aegis loop` autonomously closes
rtmx requirements; **EGRESS=0 across the whole process group**; and we have an
intent-bench profile placing the local stack against Sonnet-4. The keystone risk
is model **capability × speed** on local hardware — proven by RUNQ-004.

## 1. SERVE bake-off — the model + serving decision

The original `SERVE` stack choice (Gemma-4-26B-A4B vs Qwen3.6-35B-A3B; Ollama
spike → llama.cpp production) was never resolved. A small model (phi4-mini) is too
weak for tool calls; a big one (gemma4:26b) is accurate but slow / output-capped.

- **REQ-SERVE-016** — a bake-off over ≥3 candidates, scored on **task completion**
  (does `aegis run` actually edit files + pass the task's tests) and **WCR/TCR**
  (latency/tokens). Winner recorded in config/calibration.
- **REQ-SERVE-017** — bring up the **production** path: `llama.cpp` `llama-server`
  serving the selected GGUF (OpenAI-compatible, calibrated), validated at parity
  with the Ollama spike (preflight + a real completion).

## 2. RUNQ — run-hygiene (makes `aegis run` actually usable)

`aegis run` wiring works, but a real task hasn't completed (weak model → prose
instead of tool calls; capable model → unbounded time / `reason:"length"`).

- **REQ-RUNQ-001** — `aegis run` enforces a **wall-clock budget/timeout**
  (`--timeout`/`--budget` + default); on expiry the `opencode run` child is killed
  and a **partial transcript** is written; exit code reflects the timeout.
- **REQ-RUNQ-002** — configure the agent/system so **small local models emit real
  tool calls**, not prose (the failure we observed).
- **REQ-RUNQ-003** — set **output/step limits** so a capable model **completes**
  rather than truncating (`step_finish reason != length`).
- **REQ-RUNQ-004** *(keystone, gated)* — a **real coding task completes**:
  `aegis run` edits files and the task's tests pass on the selected model.

## 3. ENCLAVE — enclave-validation (the real target)

- **REQ-ENCLAVE-001** — prove **EGRESS=0 across the whole process group**
  (aegis + opencode + the model server + rtmx), not just the aegis binary — the
  egress gate must observe every child during a full `run`/`loop`.
- **REQ-ENCLAVE-002** — a **stage-then-disconnect runbook**
  (`docs/enclave-deployment.md`): stage on a connected host (build opencode, pull
  the model, vendor) → transfer one artifact set → run with networking disabled.
- **REQ-ENCLAVE-003** — a **closed-host smoke** (`scripts/enclave-smoke.sh`) on a
  network-disabled host: install → bare `aegis` TUI → `aegis run` completes →
  `aegis loop` closes a requirement; EGRESS=0 throughout.

## 4. REL — release-v1

- **REQ-REL-001** — **provision a signing key** (minisign/GPG), commit its public
  key to `deploy/release/`, sign `SHA256SUMS`, `make verify-release` passes.
  (Releases are currently unsigned; the tooling exists — this provisions the key.)
- **REQ-REL-002** — a tagged **v1.0** bundling the **working opencode**
  (multi-platform, via OC-007) + SBOM + checksums + signature.
- **REQ-REL-003** — an **operator guide** (`docs/operator-guide.md`): install from
  artifacts, load the model, launch TUI / `run` / `loop`, verify egress.

## 5. Critical path

SERVE-016 (model) → RUNQ-001..003 (run hygiene) → **RUNQ-004 (real task, the
usable gate)** → BENCH-003..005 (profile vs Sonnet-4) → REL-001/OC-007/REL-002
(signed multi-platform v1.0) → ENCLAVE-001..003 (closed-host validation) → SURFACE-004
(polish). Exit when the §0 definition of done holds.
