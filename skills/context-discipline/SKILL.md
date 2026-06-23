---
name: context-discipline
description: Keep context lean for a small, CPU-bound local MoE (~4B active) — prefer LSP/grep/symbol lookup over dumping whole files, scope to one requirement, spend tokens like they're metered. Use this whenever you gather context to work a requirement: reading code, searching, deciding what to put in the prompt. Trigger it any time you're about to read a whole file, paste large output, or widen scope beyond the one requirement in hand.
---

# context-discipline

The model in this stack is a small MoE with ~4B active parameters, served CPU-bound on the
Ryzen (Metal on the Mac). It is bandwidth-bound, not knowledge-bound: every extra token of
context is decode time on a busy memory bus and dilutes attention the model can't spare.
And tokens are *measured* — TCR (token cost per requirement) and WCR (wall-clock per
requirement) are tracked CI metrics (`CLAUDE.md` §5). Lean context isn't tidiness; it moves
the dashboard.

## The discipline

- **Look up, don't dump.** Prefer LSP/symbol lookup and `grep` to find the one function,
  type, or call site you need. Pull the relevant span, not the whole file. Whole-file reads
  are the default tax on a small model — pay it only when you truly need the file.
- **Scope to one requirement.** The active requirement's acceptance test defines what
  context is relevant. Anything outside it is noise that costs tokens and invites
  off-scope changes — which `build-to-spec` forbids anyway.
- **Minimize tokens deliberately.** Trim tool output before it re-enters context; don't
  echo large logs back to the model; summarize where a summary suffices. Fewer turns of
  smaller context closes requirements faster (MTC, WCR) and cheaper (TCR).
- **Narrow beats broad here.** A large-context model can afford to wander; this one can't.
  Bandwidth is the binding constraint, so precision of retrieval is the lever.

## Why it's a metric, not a preference

WCR and TCR are CPU-bound signals: on this hardware, context length translates almost
directly into decode latency and token cost. MTC (mean turns-to-close) compounds it — each
wandering round-trip reloads context. Tight retrieval is the cheapest way to hold all three
down without touching the model or harness.

## The headroom boundary

There is a deferred decision (`CLAUDE.md` §9) to bolt on `headroom`'s deterministic
compressors *only if* agentic round-trips prove a measured bottleneck — watch MTC. Until
that trigger fires, the answer is human discipline: retrieve narrowly. Don't pre-emptively
reach for a compression engine; first spend fewer tokens.

## Example

Working `RTMX-003` (verify writes status back). Wrong: read all of `internal/rtmx/` into
context "to understand it". Right: `grep` for the writeback path and the verify entry point,
read just those two spans plus `rtmx/writeback_test`, implement against them. A few hundred
relevant tokens instead of thousands — lower TCR, lower WCR, and the model stays on the one
thing the test checks.
