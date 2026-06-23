---
name: rtmx-loop
description: Talk to rtmx safely — list the next requirement, claim it atomically, do the work, write the verify result back to the CSV, and release. Use this whenever the loop touches rtmx: `rtmx next`, claim/release, `rtmx verify`, status writeback, or recovering claims after an interruption. Trigger it before any step that reads or mutates requirement state, and any time you are tempted to hold more than one claim or skip the writeback.
---

# rtmx-loop

rtmx is the source of truth: a static Go binary over a CSV-in-git database, exposed as a
stdio MCP server. aegis-cli is a *client* of it (`internal/rtmx`). The loop's correctness
rests entirely on touching that state safely — one requirement at a time, claims that
survive a crash, and a verify result that actually lands in the CSV. Get this wrong and
the audit trail lies.

## The contract with rtmx

- **One claim at a time.** The loop holds exactly one requirement between claim and
  release. Never claim a second while one is open — that is how two runs (or two retries)
  collide on the same work and the CSV ends up inconsistent.
- **Atomic claim/release (`RTMX-002`).** Claiming is a compare-and-set on the requirement's
  status: it succeeds only if the requirement is still available. A double-claim must fail
  loudly, not silently overwrite. Release is equally atomic — a half-released claim is a
  leaked claim.
- **Verify writes status back (`RTMX-003`).** `rtmx verify` runs the requirement's linked
  test and writes pass/fail to the CSV. The writeback *is* the close — an in-memory "it
  passed" that never reached the CSV did not happen. Confirm the write landed before
  releasing.
- **Resumability (`LOOP-003`).** A claim is durable state in rtmx, not in aegis-cli's
  memory. After an interruption, the open claim is still there; the loop reattaches to it
  rather than re-claiming or orphaning it. Recover, then continue — never start clean over
  a live claim.

## Transport: MCP stdio, CLI fallback

Prefer the stdio MCP server — it is the structured, long-lived path and keeps tool calls
machine-checkable. Keep a CLI fallback (`rtmx next`, `rtmx verify`, …) for when MCP is
unavailable: same operations, same atomicity guarantees, shelling the binary instead. The
fallback is a transport swap, not a behavior change — both honor one-claim-at-a-time and
both write back. Both stay on the local box; rtmx makes no network call.

## What this skill does NOT do

It moves requirement *state*; it never invents requirement *content*. Authoring or
splitting requirements is `decomposition`, and it is human-gated. The loop claims, works,
verifies, releases — it does not edit acceptance criteria to make a verify pass.

## Example

Loop iteration: `rtmx next --one` returns `LOOP-005`; the client claims it (compare-and-set
on its status — succeeds, no one else holds it); the harness produces a diff; `rtmx verify`
runs `loop/drain_test`, passes, and writes `closed` back to the CSV; the client confirms the
writeback and releases. Crash mid-work? On restart the `LOOP-005` claim is still in rtmx —
the loop reattaches and resumes instead of double-claiming.
