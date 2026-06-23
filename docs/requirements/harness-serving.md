# Requirement Specification — Built-in Serving-Backed Harness

**Thread:** `HARNESS-003..010`, `FEAT-008..010` · **Phase 3 / sprint v0.3** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `build-to-spec`, `context-discipline`, `airgap-hygiene`, `rtmx-loop`

## 1. Purpose & scope

Today aegis-cli can call a local model (`internal/serving/client.go`) and run the
control loop, but the production harness adapters (`internal/harness/opencode`,
`goose`) spawn nothing — the only real driver is a serving-backed *test* adapter
in `test/bdd`. This thread promotes that into a **first-class, built-in
serving-backed harness** (`internal/harness/serving`) so aegis-cli can generate
code end-to-end against a real loopback model **without any external harness
binary**. After this thread, pointing aegis-cli at a real `llama-server`/Ollama
on loopback is a config change, not new code.

In scope: prompt construction, model invocation, edit parsing, sandboxed +
atomic edit application, acceptance-test execution, metrics, harness selection,
and E2E coverage. Out of scope: tool-calling/MCP harness behavior (that remains
opencode/goose's domain), multi-file refactors beyond unified-diff/file-block
edits, and any non-loopback network path (forbidden by construction).

## 2. Definitions

- **Edit set** — the structured changes parsed from a model response: a unified
  diff or one or more fenced `path + content` file blocks.
- **Workspace** — the repository working tree the harness is permitted to modify.
  Writes resolving outside the workspace root are rejected.
- **Attempt** — one generate→apply→test cycle. The loop allows `Retries+1`
  attempts per requirement before parking (`internal/loop`).

## 3. Requirements

Each requirement is well-formed (EARS style), independently verifiable, and
linked to the acceptance test that closes it via `rtmx verify`.

### REQ-HARNESS-003 — Serving-backed adapter drives a requirement
**The built-in harness shall** implement `harness.Adapter` and, on `Drive`,
obtain an edit from the local model via the loopback serving client and return a
populated `harness.Diff`.
*Rationale:* the loop already consumes `harness.Adapter`; this is the real
implementation behind that seam.
*Acceptance:* given a loopback endpoint returning a valid edit, `Drive(req)`
returns a `Diff` with `RequirementID==req.ID` and a non-empty `Patch`, making no
non-loopback call. *Test:* `internal/harness/serving::TestServingAdapterDrives`.
*Depends on:* REQ-HARNESS-001, REQ-SERVE-006.

### REQ-HARNESS-004 — Lean, scoped prompt
**The harness shall** build the model prompt from the requirement's id, text, and
acceptance-test references plus only the minimal repo context needed, and **shall
not** dump unrelated files.
*Rationale:* the local MoE is bandwidth-bound; TCR/WCR are tracked. See
`context-discipline`.
*Acceptance:* the built prompt contains the requirement id + text + its test
ref(s) and excludes files not referenced by the requirement.
*Test:* `internal/harness/serving::TestBuildPromptIsScoped`. *Depends on:* 003.

### REQ-HARNESS-005 — Robust edit parsing with retry
**When** the model response is malformed or only partially parseable, **the
harness shall** detect it and retry within the attempt budget rather than crash;
**when** the response is a valid edit set, it shall parse cleanly.
*Rationale:* small models emit malformed output; inherits REQ-HARNESS-002's
discipline.
*Acceptance:* a malformed-then-valid response sequence yields a parsed edit set
and records the retry; an irrecoverable response surfaces a typed error, not a
panic. *Test:* `internal/harness/serving::TestParseEditsRetriesOnMalformed`.
*Depends on:* 003, REQ-HARNESS-002.

### REQ-HARNESS-006 — Workspace-sandboxed application
**The harness shall** reject any edit whose target path resolves outside the
workspace root (absolute paths, `../` traversal) **before** writing anything.
*Rationale:* a weak model must never be able to write outside the repo; ITAR
host-safety. *Acceptance:* an edit targeting `/etc/x` or `../../x` is refused and
no file is written. *Test:*
`internal/harness/serving::TestApplyRejectsOutsideWorkspace`. *Depends on:* 003.

### REQ-HARNESS-007 — Atomic application with rollback
**If** applying an edit set fails partway, **or** the post-apply acceptance test
fails, **the harness shall** restore the working tree to its pre-attempt state.
*Rationale:* a bad attempt must not corrupt the repo; clean rollback enables
retry and resumability. *Acceptance:* after a failed apply/test, every touched
file matches its pre-attempt content. *Test:*
`internal/harness/serving::TestApplyRollsBackOnFailure`. *Depends on:* 006.

### REQ-HARNESS-008 — Acceptance-test execution + metrics
**The harness shall** run the requirement's acceptance-test command after a
successful apply and reflect pass/fail, and **shall** populate the `Diff`
metrics (`Turns`, `ToolCalls`, `ValidToolCalls`, `Tokens`).
*Rationale:* feeds the loop's verify decision and the dashboard (`metrics-eval`).
*Acceptance:* a passing edit yields a `Diff` with non-zero metrics and a green
test run; a failing edit reflects the failure. *Test:*
`internal/harness/serving::TestRunsTestsAndReportsMetrics`. *Depends on:* 003.

### REQ-HARNESS-009 — Loopback-only egress
**The built-in harness shall** make zero non-loopback network calls; a
non-loopback endpoint shall be refused at construction.
*Rationale:* the air-gap non-negotiable expressed at the harness layer.
*Acceptance:* construction against a non-loopback endpoint errors; a run produces
no non-loopback egress under `scripts/verify-airgap.sh`. *Test:*
`internal/harness/serving::TestServingHarnessLoopbackOnly`. *Depends on:* 003.

### REQ-HARNESS-010 — Configurable harness selection
**The configuration shall** select the harness implementation
(`builtin` | `opencode` | `goose`), and **`aegis run` shall** wire the
built-in serving-backed harness when `builtin` is selected.
*Rationale:* keep the harness swappable behind config, per CLAUDE.md §2.
*Acceptance:* `Harness="builtin"` routes the loop to the serving harness;
`opencode`/`goose` remain selectable; an unknown value is rejected by config
validation. *Test:* `internal/config::TestHarnessSelectionBuiltin`.
*Depends on:* 003.

### REQ-FEAT-008 — E2E: built-in harness closes a requirement
**Scenario (Gherkin, `features/serving_harness.feature`):** a loopback model
emits a valid edit → applied → the requirement's test passes → the requirement is
closed by verify. *Test:* `test/bdd::TestFeatures`. *Depends on:* 007, 008.

### REQ-FEAT-009 — E2E: malformed response is retried then closes
**Scenario:** the first model response is malformed; the harness retries; the
second succeeds and the requirement closes. *Test:* `test/bdd::TestFeatures`.
*Depends on:* 005.

### REQ-FEAT-010 — E2E: out-of-workspace edit is rejected, run parks safely
**Scenario:** the model proposes an edit outside the workspace; it is refused;
the working tree is untouched; the requirement parks rather than corrupting the
repo. *Test:* `test/bdd::TestFeatures`. *Depends on:* 006.

## 4. Design constraints

- Std-lib only in the shipped path (enforced by `test.TestRuntimeBinaryIsStdLibOnly`).
- New package `internal/harness/serving`; selection wired in `internal/config` +
  `cmd/aegis`/`internal/loop` construction. No edits to the loop's contract.
- Edit application operates on a copy/snapshot to honor REQ-HARNESS-007; prefer
  OS primitives over a new VCS dependency.
- Generation and verification stay phase-separated (loop already enforces this).

## 5. Verification & exit criteria

The thread is complete when all eleven requirements are `COMPLETE` via
`rtmx verify --update`, `rtmx health` is HEALTHY at 100% coverage, and `make ci`
is green (race, lint, govulncheck, cover-gate ≥ floor, EGRESS=0, TRACE=100%,
ACR-regression). Build order follows the dependency graph: 003 → 004/005/006/008/
009/010, then 007, then the FEAT E2E scenarios.
