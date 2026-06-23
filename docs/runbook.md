# Operator Runbook — aegis-cli

How to operate the requirements-driven build loop day to day. This is the
in-enclave operator's reference; read `CLAUDE.md` for architecture and the
relevant `skills/` before changing behavior.

## Preconditions

Before any run, the closed environment must be verified:

```bash
aegis verify-env          # reports egress + traceability status (must be clean)
make health               # rtmx health — TRACE=100%, no orphans (hard gate)
```

A run must never start if `verify-env` reports reachable egress or `rtmx health`
is not HEALTHY. Both are build-failing conditions by design.

## One-time host setup

```bash
make hooks-install        # install pre-commit (make ci-fast) + pre-push (make ci)
scripts/bench.sh --model /models/<your-model>.gguf   # calibrate serving to THIS host
```

Calibration writes `deploy/llama-server/calibration.json` (with a `target`
field). An uncalibrated serving launch is a hard error — calibrate first. See
`skills/serving-calibration`.

## The loop

aegis-cli claims one rtmx requirement, drives the harness to implement + test
it, verifies, and releases — then moves on. One requirement at a time.

```bash
# single iteration (interactive, watch one requirement close)
aegis run --once

# drain the backlog unattended, bounded (park-on-escalation, breaker, budget)
aegis run --max 40 --break-after 3
```

See `skills/rtmx-loop`, `skills/build-to-spec`, and `skills/unattended-operation`.

## Watching progress

```bash
make rtm                  # full RTM status
make backlog              # prioritized backlog (what runs next)
rtmx context              # token-efficient summary (blockers + quick wins)
```

## Closed-loop verification

Requirement status is never hand-edited to COMPLETE. It is written back from a
passing test run:

```bash
rtmx verify --dry-run     # preview status changes from the current test results
rtmx verify --update      # run tests, write COMPLETE/PARTIAL back to the database
```

A requirement closes only when its mapped `test_module`/`test_function` passes.
Autonomy is bounded by those tests — a weak test lets the model close work the
wrong way. Trust the backlog's tests before a long run.

## Human-gated decomposition

The loop never authors its own work. To split a coarse requirement:

```bash
aegis propose LOOP        # emits atomic children in a `proposed` state (not claimable)
```

A human reviews, tightens acceptance tests, and approves before children become
claimable. See `skills/decomposition`.

## On return from an unattended run

Review the audit log and metrics — do not infer success from "the backlog is
empty". Check: what closed (by verify), what parked and why, whether the breaker
tripped, and the dashboard trend (ACR is the north star). See `skills/metrics-eval`.

```bash
python3 scripts/ci-metrics.py --golden eval/golden --baseline eval/baseline.json
```

## Pre-commit / pre-push parity

The Makefile is the single source of truth for the pipeline. `make ci` runs the
full gate (build → unit → airgap → health → metrics); the pre-push hook runs the
identical `make ci`; pre-commit runs the fast subset `make ci-fast`. GitHub
Actions runs the same `make ci`. You can always reproduce CI exactly:

```bash
make ci                   # the exact pipeline CI runs, locally
```

## When something breaks

- **Endpoint unhealthy / model regressed** — the circuit breaker halts the run
  after M consecutive failures. Stop, check `serving` health, re-`bench.sh` if
  the host changed. Do not grind the backlog through a broken endpoint.
- **A requirement keeps failing verify** — it parks (blocked + logged + claim
  released). Read the audit log; the test or the spec is usually the issue, not
  the model. Fix the spec or the test — do not loosen the test to pass.
- **Egress detected** — stop immediately. This is the ITAR control. Find the
  component making the call (`scripts/verify-airgap.sh`), close it, re-verify.
