---
name: discovery
description: Integrate continuous discovery/framing (design thinking) into the rtmx delivery loop. Use this whenever deciding WHAT to build next, before authoring requirements, when reviewing parked work or metrics to reframe, or any time you are tempted to let the loop invent its own work. Pairs with decomposition (the split mechanics) — this owns the framing discipline.
---

# discovery

The loop is convergent: decompose to an atomic decision, close it with a test,
terminate in functional value. That guarantees efficient *delivery* — never that
you are building the *right* thing. Discovery/framing is the divergent front that
decides what is worth building. It is **human-led**; the machine assists. The two
run as a coupled double loop, never one autonomous pipeline.

```
   DISCOVERY (human-led, divergent)            DELIVERY (loop-led, convergent)
   frame problem / outcome / user                  drain atomic, test-linked reqs
   → coarse requirement + spec doc   ──propose──▶    one decision → functional value
        ▲                                                │
        └──────── evidence: parked reqs, ACR/MTC, audit ◀┘
```

## The discipline

- **The spec doc is the define gate.** No requirement enters the backlog without
  a `docs/requirements/<thread>.md` stating problem → outcome → acceptance, linked
  from the `requirement_file` column. Design thinking's *define* stage, made an
  admission gate. `aegis frame` flags anything unframed.
- **Frame in vertical slices.** The unit of framing is a `FEAT-*` slice that
  delivers user-observable value end to end — never a horizontal "build the
  parser" layer. Every workstream terminates in functional value, so frame the
  next *thinnest valuable slice*, not the next component.
- **Delivery evidence is the discovery backlog.** Parked (blocked) requirements
  mean the spec was ambiguous or wrong — they are reframe inputs, not failures.
  ACR/MTC/TCVR trends and the audit log are the empathize/define signals for the
  next cycle. Run `aegis frame` to surface them; see `metrics-eval`.
- **The machine proposes; the human frames and approves — always.** `aegis
  propose` decomposes a framed outcome into atomic children and can draft
  acceptance tests, but a human owns intent and admits work. Closure stays
  verify-driven; no command marks a requirement done. This is the guardrail that
  keeps discovery from becoming the closed self-validating loop the architecture
  exists to prevent (see `decomposition`).

## The ritual (continuous, not a phase gate)

Interleave with delivery; the loop never blocks on it. Periodically: (1) run
`aegis frame` and read the reframe + unframed lists; (2) reframe parked work and
trace unframed work to an outcome; (3) `aegis propose` the next atomic slices for
human approval. The loop drains what is approved while framing runs alongside.

## Example

Overnight, the loop parks `REQ-RUN-009` (verify kept failing) and ACR dips. Morning
`aegis frame` surfaces it in the reframe backlog. You realize the acceptance test
encoded the wrong outcome — a framing error, not a model failure. You rewrite the
spec slice, `aegis propose` two tighter atomic children, approve them, and the loop
delivers them that afternoon. Discovery closed the loop; the human kept the say.
