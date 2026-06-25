# Requirement Specification — Setup Orchestrator (thin shim + Python)

**Thread:** `SETUP-001..006` · **Phase 9 / sprint v1.0** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `airgap-hygiene`, `build-to-spec`

## 0. Why

`setup.sh` has grown a real UI (toolchain bootstrap, a model menu, a progress
bar) inline in bash. The next features we want — a last-N-lines panel and
determinate progress — are genuine UI engineering, and bash is the wrong tool:
hard to test, hard to keep DRY, brittle around terminal control. Shift the
**orchestration + UI into std-lib Python**, keep **`setup.sh` a thin shim**, and
keep the **actual work in the existing shell scripts** (so it stays shared with
`make ci-full` and `release.sh`). This makes setup durable, rugged, idempotent,
and testable.

## 1. Architecture

```
setup.sh                 # THIN shim: ensure python3, exec the orchestrator, pass args
scripts/setup/           # std-lib Python ONLY (no pip deps — air-gap)
  main.py                # arg parsing + orchestration entrypoint
  orchestrator.py        # sequence steps; idempotency, resume, status-aware next-steps
  steps.py               # declarative Step units: title, is_done(), run(), progress source
  ui.py                  # terminal UI: bars (determinate/indeterminate), last-N-lines panel,
                         #   menu, colour; tty / NO_COLOR / --verbose aware. Pure where possible.
  catalog.py             # model catalog + selection menu (migrated from MODEL-003)
  profile.py             # per-step duration cache -> time-estimate progress
```

**The work stays in the shell scripts** — `scripts/{build-opencode, build-llama,
stage-model, pin-model, fetch-model, bench, integration-smoke}.sh`. The
orchestrator runs them as subprocesses (output → `setup.log`) and renders the UI;
it never reimplements a build. Those same scripts are what `make ci-full` and
`release.sh` already call — **one source of the work** (DRY).

**Separation of concerns:** `steps.py` never draws UI; `ui.py` never runs steps;
`orchestrator.py` wires them. Each is unit-testable in isolation.

## 2. Requirements

- **SETUP-001 — Thin shim.** `setup.sh` (< ~25 lines) verifies/guides `python3`
  and `exec`s `scripts/setup/main.py "$@"`. No phases/menu/progress in bash.
- **SETUP-002 — Orchestrator.** A std-lib-only Python orchestrator sequences the
  steps with step logic isolated from UI in separate modules; no pip deps.
- **SETUP-003 — Idempotent + resumable.** Each `Step.is_done()` gates `run()`; a
  fully-built tree re-runs as all-skips; an interrupted run resumes without manual
  cleanup.
- **SETUP-004 — Rugged.** A step failure is isolated (clear error + log path +
  next-step), leaves no corrupt/partial state (`.part`/half-clones cleaned or
  resumable), and the run stays re-runnable; exit codes are meaningful.
- **SETUP-005 — DRY.** Steps reuse the existing build/stage scripts (the ones
  `make ci-full`/`release.sh` use); zero build logic copied into Python; the UI is
  one module.
- **SETUP-006 — UI module.** Isolated, capability-aware, unit-tested: determinate
  % from download bytes / cmake `[N%]` / a profile time-estimate, else a bouncing
  bar; a last-N-lines panel; pure render functions tested; non-tty + `--verbose`
  degrade cleanly.

## 3. The Step contract (the DRY/idempotency core)

Each step is declarative:

```python
class Step:
    title: str
    def is_done(self) -> bool: ...        # idempotency gate (binary exists + pin matches, etc.)
    def run(self, ui) -> int: ...         # subprocess a shell script; report progress to ui
    def progress(self, log_tail) -> float|None:  # 0..100 or None (indeterminate)
```

- `is_done()` makes re-runs cheap + resumable (SETUP-003): e.g. `deploy/opencode/
  bin/opencode` present, `llama-server --version` ok, model staged + sha matches
  `MODEL_REF`, `calibration.json` present.
- `run()` shells out to the dedicated script (SETUP-005) — never duplicates it.
- `progress()` returns a real % when a source exists (download bytes vs
  `catalog.size`; parse cmake `[N%]`; `profile` time-estimate), else `None` →
  the bouncing bar (SETUP-006).

## 4. Qualities (durable / rugged / idempotent / DRY)

- **Durable / rugged:** per-step `try/except`; a failure prints the error + the
  `setup.log` path + the next action, and does not falsely fail unrelated phases;
  partial downloads/clones are cleaned or resumable; the process restores the
  cursor on exit/`SIGINT`.
- **Idempotent:** `is_done()` everywhere; no destructive redo; safe to Ctrl-C and
  re-run.
- **DRY:** the work lives once (shell scripts), the UI lives once (`ui.py`), steps
  are declarative data — adding a step is one `Step`, not new UI/flow code.
- **Air-gap:** std-lib only (no pip); only the build/download steps touch the
  network, and only on the connected host.

## 5. Verification (bridging Python tests into `rtmx verify`)

`rtmx verify` maps Go tests. So each SETUP requirement maps to a Go test in
`test/` that either **inspects structure** (shim is thin, modules exist, steps
call the scripts) or **bridges** (`exec python3 -m unittest discover scripts/setup`
and asserts exit 0). The Python modules carry their own `unittest` suites for the
pure logic (UI rendering, progress math, `is_done` gating, ruggedness).

## 6. Exit criteria

SETUP-001..006 COMPLETE via `rtmx verify`; `rtmx health` HEALTHY; `make ci` green.
`setup.sh` is a thin shim; `scripts/setup/` is std-lib Python with isolated
UI/steps; `make ci-full`/`release.sh` still call the same shell scripts (no
duplication); a re-run on a built tree is all-skips; a killed run resumes cleanly.
