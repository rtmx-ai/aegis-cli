---
name: unattended-operation
description: Run the backlog unattended — drain it continuously, park on escalation instead of waiting, break the circuit on repeated failure, and stay inside a run budget. Use this whenever the loop runs without a human watching: overnight runs, CI-driven drains, "leave it working while I'm away", or any `aegis run` without `--once`. Trigger it before starting an unattended session and when deciding how the loop should behave on failure, escalation, or a stuck requirement.
---

# unattended-operation

The loop is safe to run unattended on a fixed, well-specified backlog — that is what
resumability (`LOOP-003`), atomic claim/release (`RTMX-002`), and the audit log are for.
But "unattended" changes how failure and escalation must behave: nobody is there to
unblock a stuck requirement or notice the run going sideways. Four behaviors make it safe.

## The four behaviors

- **Continuous drain.** `aegis run` (without `--once`) pulls `rtmx next` and works
  requirements until the backlog is empty or a stop condition fires. One requirement at a
  time, same discipline as a single iteration.
- **Park on escalation, don't wait.** Unattended, `LOOP-002`'s escalation can't mean
  "block until a human answers." It means: mark the requirement blocked, log why, release
  the claim, and move to the next. One hard requirement must never stall the whole night.
- **Circuit breaker.** Stop the run after M consecutive failures. Consecutive failures
  mean something broke — the endpoint died, the model regressed, a dependency moved — and
  grinding through the rest just produces garbage you'll have to unwind. Stop and surface.
- **Run budget.** Cap the session: max requirements and/or wall-clock. A bounded run is a
  reviewable run; an unbounded one is how a calibration drift quietly burns a night.

All four are config (`internal/config`), read by `internal/loop`. Defaults are
conservative: small consecutive-failure threshold, a finite budget, park-not-wait on.

## Autonomy is bounded by your tests

The loop's safety ceiling is the acceptance tests behind each requirement. A weak or
lenient test lets the model "close" a requirement the wrong way, and `rtmx verify` won't
catch what the test doesn't check. So unattended autonomy is only as trustworthy as the
tests — before a long run, trust the backlog's tests, not the model. When in doubt, shrink
the run and review the audit log before extending it.

## What unattended operation does NOT do

It executes an existing backlog. It never authors or decomposes requirements on its own —
that is `decomposition`, and it is human-gated by design. An unattended run that invented
its own work, then graded itself against criteria it also invented, is exactly the closed
self-validating loop the architecture exists to prevent.

## On return

Everything is in the audit log and the metrics. Review: what closed, what parked (and
why), whether the breaker tripped, and the dashboard trend. The run reports a summary;
the log is the record. Do not infer success from "the backlog is empty" — infer it from
the verify results and the parked list.

## Example

`aegis run --max 40 --break-after 3` overnight: drains up to 40 requirements, parks any
that exhaust retries (logged, claim released, next pulled), and halts immediately if three
in a row fail. Morning review: 31 closed, 6 parked on under-specified tests, breaker never
tripped — a clean, bounded, reviewable night.
