---
name: decomposition
description: Break a coarse requirement into atomic children — as PROPOSALS a human approves, never as work the machine authors and then grades itself on. Use this whenever a requirement is too big to close in one scoped pass, when splitting work into atomic units, or when running `aegis propose`. Trigger it any time the agent is tempted to invent sub-requirements, write its own acceptance criteria, or decide on its own what "atomic" means.
---

# decomposition

The reason a weak local model is trustworthy in this stack is that scope and acceptance
criteria come from human-authored, test-linked requirements. Decomposition is the one
place that principle is most at risk, because splitting work *feels* like authoring work.
The rule holds anyway: **the machine proposes decompositions; a human approves them.** The
model is a consumer of intent, never the author of it.

## Why this is gated, not autonomous

If the loop could add and approve its own atomic requirements, it would decide what
"atomic" means, write its own acceptance criteria, and then grade itself against criteria
it just invented — a closed, self-validating loop with no external ground truth. That is
the exact failure mode the architecture was built to prevent. It is also the worst task to
hand a 4B-active model unattended: decomposition is long-horizon, ambiguous design
reasoning, the region where the model is weakest. And for ITAR, a system that authors its
own work on controlled data with nobody reviewing what it decided to build weakens the
human-accountability story the Technology Control Plan depends on.

## The flow

1. `aegis propose <requirement>` reads a coarse requirement and emits atomic children.
2. Children land in a **`proposed` state — not claimable.** The loop's `rtmx next` must
   skip them. (This needs an rtmx convention: a `proposed` status, or a tag `next`
   excludes, that only a human approval promotes to active.)
3. Children **inherit the parent's acceptance tests** rather than inventing new criteria.
   Where a child genuinely needs a narrower test, it is flagged for the human to write —
   the machine does not author the bar it will be graded against.
4. A **depth limit and a child cap** bound the split — no runaway trees, no backlog
   explosion. Exceed either and `propose` stops and asks for a human decision.
5. Every proposed requirement is tagged **machine-authored** in the audit trail, so
   provenance is always visible on review.
6. A human reviews and approves; only then do children become claimable by the loop.

## The division of labor

The machine does the tedious splitting — pulling apart a coarse requirement into plausible
atomic pieces, wiring up inheritance, flagging gaps. The human keeps authority over intent
— what the pieces are, what "done" means, which to admit. Assistive, not autonomous. That
preserves the design's integrity instead of inverting its core principle.

## Example

`aegis propose LOOP` on a coarse "make the loop production-ready" → emits proposed children
(drain, park-on-escalation, circuit breaker, run budget), each inheriting the parent's
tests, each tagged machine-authored, all in `proposed` state, none claimable. You review,
tighten two acceptance tests, approve all four. Now the loop can pick them up. The machine
saved you the breakdown; you kept the say over what's real.
