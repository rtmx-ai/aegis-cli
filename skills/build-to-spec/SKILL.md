---
name: build-to-spec
description: The core implementation discipline — claim ONE requirement, make the minimal change that satisfies its acceptance test, run the test, verify, release. Use this every time you implement a requirement: it is the inner loop. Trigger it whenever you are about to write code, and especially when you feel the pull to fix a nearby thing, refactor "while you're here", or tweak a test that's in your way.
---

# build-to-spec

This is the implementation discipline at the heart of the loop. A requirement is a closed
question with a test for an answer. Your job is to make that one test pass with the
smallest change that honestly does so — then verify and release. Nothing more. The whole
architecture (one requirement at a time, a small local model, independently verifiable
changes) depends on this staying small.

## The discipline

1. **Claim one requirement.** Exactly one, via `rtmx-loop`. Its acceptance test is your
   whole scope.
2. **Read the test first.** The linked test is the spec made concrete. Understand what it
   actually checks before writing a line — that tells you the minimal change.
3. **Make the minimal change.** Smallest diff that makes the test pass for the right
   reason. No adjacent refactors, no speculative abstraction, no "I'll also fix…".
4. **Run the test, then `rtmx verify`.** Local run for the fast loop; verify is the
   authoritative close that writes status back.
5. **Release and move on.** Done means closed-by-verify and released — not "looks done".

## The acceptance test is the contract

The test is ground truth, not an obstacle. If a test is in your way, you do **not** edit it
to make your change pass — that inverts the entire trust model, where humans own the bar
and the machine clears it. Two honest outcomes only: make your code satisfy the test, or if
the test is genuinely wrong, **fix the spec** — flag it for a human, change the requirement
and its criteria deliberately, never quietly weaken the assertion to game a green.

## Don't expand scope

Scope is narrowed on purpose so a ~4B-active model can succeed and so every change is
reviewable in isolation. Expanding scope — bundling two requirements, refactoring beyond
the diff, gold-plating — defeats both. If you discover real adjacent work, it is a *new
requirement* for rtmx (human-gated via `decomposition`), not extra cargo on this one.

## Example

Claim `HARNESS-002` ("malformed tool call is detected + retried, not crashed"). Read
`harness/toolcall_test`: it feeds a bad call and expects a retry, not a panic. Minimal
change: wrap the decode, classify malformed-call, route to the existing retry path. You
notice the harness adapter could be refactored — leave it; not this requirement. Run the
test (green), `rtmx verify` (closed, written back), release. One small, reviewable diff.
