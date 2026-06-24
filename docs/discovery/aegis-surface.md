# Discovery — aegis's external surface vs. its inner surfaces

**Status:** framing (discovery) · **Skill:** `skills/discovery` · **Date:** 2026-06-24

## The question

aegis is a facade over three tools, each with its own CLI/API grammar: **rtmx**
(intent), **opencode** (harness), **ollama** (model). What should aegis's *outer*
surface be? Should it be consistent with — and expose — the inner surfaces, and
if so, which ones?

## What the inner surfaces actually are

Recurring verbs across the stack (the latent shared grammar):

| Concept | rtmx | opencode | ollama |
|---|---|---|---|
| do work | `next` | `run <msg>` | `run <model>` |
| serve | `mcp-server` | `serve` | `serve` |
| inventory | `backlog`, `deps` | `models`, `session` | `list`, `ps` |
| health | `health`, `hygiene` | `stats` | `ps` |
| intent | `next/claim/verify` | — | — |
| mcp | `mcp-server` | `mcp` | — |
| init/config | `init`, `config`, `context` | `providers` | `create`, `show` |

`run`, `serve`, `list/models`, `status`, `mcp`, `init` appear in all/most layers.

## The tension

1. **`run` collision.** aegis `run` = *drain the rtmx backlog* (a loop). Every
   inner tool's `run` = *execute one task*. This mis-trains the operator.
2. **Facade vs. curated.** Two honest poles:
   - *Thin pass-through* — mirror each inner surface under a namespace
     (`aegis rtmx …`, `aegis code …`, `aegis model …`), hardened for air-gap.
     Maximally consistent (it *is* the inner surface); zero capability hidden.
   - *Curated grammar* — a small, opinionated verb set aegis owns
     (`run`, `serve`, `status`, `models`), each dispatching across layers.
     Predictable; but hides the long tail and must be maintained.

## Principles (proposed)

- **Predictable from the inside out.** If you know `ollama run` / `opencode run`,
  `aegis run <prompt>` should mean the same (execute one task). Same-name ⇒
  same-meaning across layers.
- **Curate the 80%, escape-hatch the 20%.** Curated top-level verbs for the
  common path; transparent pass-through namespaces for the full inner surfaces.
- **Never hide capability, only harden it.** A pass-through wraps the inner tool
  in the air-gap envelope (offline config, egress gate); it does not remove verbs.
- **aegis owns the verbs nobody else has:** the loop, framing, calibration,
  egress/trace gates — its reason to exist.

## Options

**A — Curated unified grammar.** aegis exposes one consistent verb set
(`run`/`serve`/`models`/`status`/`next`/`verify`) dispatching to the right inner
tool. Cleanest mental model; long tail unreachable without flags.

**B — Namespaced pass-through.** `aegis rtmx|code|model <anything>` mirrors each
inner surface verbatim (hardened) + aegis's own orchestration verbs. Maximal
consistency + zero hidden capability; more surface, less opinion.

**C — Hybrid (recommended).** Curated top-level verbs for the common path, with
same-name-same-meaning consistency, **plus** pass-through namespaces as the
escape hatch. Resolve the `run` collision:
- `aegis run <prompt>` → one-shot agent run (≡ opencode/ollama `run`) — the
  `aegis solve` work lands here.
- `aegis loop` (or `drain`) → the rtmx backlog drain (today's `aegis run`).
- `aegis serve` → bring up the stack (model + opencode serve, loopback).
- `aegis models` → ollama inventory (hardened). `aegis status` → unified health
  (rtmx health + endpoint + model ps). `aegis mcp` → rtmx MCP.
- `aegis rtmx|code|model …` → pass-through escape hatches.

### Proposed mapping (Option C)

| aegis verb | dispatches to | meaning |
|---|---|---|
| `aegis` (bare) | opencode (tui) | launch the hardened TUI |
| `aegis run <prompt>` | opencode `run` | one-shot agent task → transcript |
| `aegis loop` | the orchestrator | drain the rtmx backlog (was `aegis run`) |
| `aegis serve` | model + opencode | bring the stack up (loopback) |
| `aegis status` | rtmx + ollama + endpoint | unified health/inventory |
| `aegis models` | ollama | list/show models (hardened) |
| `aegis next/verify` | rtmx | intent verbs |
| `aegis frame/init/verify-env` | aegis | orchestration aegis owns |
| `aegis rtmx\|code\|model …` | pass-through | full inner surface, hardened |

## Cost / breaking change

Option C renames today's `aegis run` → `aegis loop`. That's a breaking CLI change
(pre-1.0, low blast radius) but it removes the central inconsistency and frees
`run` for its cross-layer meaning. The benchmark/solve work then lands as
`aegis run` (consistent), not a bespoke `aegis solve`.

## Open decision (for the human)

Which model — A (curated), B (pass-through), or C (hybrid)? And: accept the
`run` → `loop` rename to make `aegis run <prompt>` consistent with the inner tools?
