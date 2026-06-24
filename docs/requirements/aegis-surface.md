# Requirement Specification — aegis External Surface (Hybrid Grammar)

**Thread:** `SURFACE-001..004` · **Phase 8 / sprint v0.4** · Status: PLANNED
**Framing:** `docs/discovery/aegis-surface.md` · **Skills:** `discovery`, `go-conventions`

## 1. Purpose

aegis is the air-gapped conductor over three inner tools — **rtmx** (intent),
**opencode** (harness), **ollama** (model) — each with its own CLI grammar. Its
external surface should be **predictable from the inside out**: knowing the inner
tools should let an operator predict aegis. Discovery (the framing doc) chose a
**hybrid** surface — curated cross-layer verbs for the common path **plus**
hardened pass-through namespaces for the full inner surfaces — and resolved the
`run` collision by renaming the backlog drain to `loop`.

## 2. The grammar (decided)

```
aegis                     # launch the hardened OpenCode TUI (bare)
aegis run <prompt>        # one-shot agent task (≡ opencode/ollama run)
aegis loop [--once …]     # drain the rtmx backlog (was: aegis run)
aegis serve               # bring the stack up (model + opencode), loopback
aegis status              # unified: rtmx health + endpoint + model ps
aegis models              # ollama inventory (hardened)
aegis next | verify       # rtmx intent verbs
aegis init | frame | verify-env | propose | version   # aegis's own verbs
aegis rtmx|code|model …   # hardened pass-through to the inner tool
```

**Principle: same-name ⇒ same-meaning.** A verb aegis shares with an inner tool
means the same thing. **Never hide capability, only harden it** — pass-throughs
wrap the inner tool in the air-gap envelope (offline config, egress gate), never
remove verbs.

## 3. Requirements

### REQ-SURFACE-001 — Hybrid grammar
**aegis shall** expose curated top-level verbs (same-name-same-meaning as the
inner tools) **and** hardened pass-through namespaces. *Acceptance:* `--help`
lists the curated verbs + the `rtmx|code|model` namespaces. *Test:*
`cmd/aegis::TestSurfaceGrammar`. *Depends on:* REQ-TUI-001.

### REQ-SURFACE-002 — `run` is one-shot; drain is `loop`
**`aegis run <prompt>` shall** execute a single agent task (≡ opencode/ollama
`run`); **the backlog drain shall** move to `aegis loop` (with the former
`run` flags: `--once/--max/--break-after/--budget`). *Test:*
`cmd/aegis::TestRunOneShotAndLoopDrain`. *Depends on:* REQ-SURFACE-001, REQ-BENCH-001.
*Breaking change:* pre-1.0; documented in the README + usage.

### REQ-SURFACE-003 — Hardened pass-through namespaces
**`aegis rtmx|code|model <args>` shall** forward to the inner tool under the
air-gap environment (offline config, loopback, egress gate), hiding no capability.
*Test:* `cmd/aegis::TestPassthroughNamespaces`. *Depends on:* REQ-SURFACE-001.

### REQ-SURFACE-004 — Curated cross-layer verbs
**aegis shall** unify inventory + health across layers: `aegis status`
(rtmx health + endpoint + model ps), `aegis models` (ollama inventory, hardened),
`aegis serve` (bring the stack up, loopback). *Test:*
`cmd/aegis::TestCuratedCrossLayerVerbs`. *Depends on:* REQ-SURFACE-001.

## 4. Cross-thread impact

- **OC-002 re-pointed:** build the **classic** `packages/opencode` CLI (ships
  headless `opencode run` + the documented provider config), not the 2.0-preview
  `packages/cli` (`lildax`, no `run`, stubbed HTTP run).
- **BENCH-001 re-pointed + unblocked:** `aegis run <prompt>` drives
  `opencode run --pure --format json` (validated executing against Ollama), not
  the unimplemented serve `/wait`. The serve client (`serve.go`) is retained for
  when upstream lands the HTTP run.
- **RenderConfig** adjusts to the classic schema (drop the rejected
  `offline`/`telemetry` JSON keys; air-gap stays enforced via env markers,
  `--pure`, and the egress gate).

## 5. Verification & exit criteria

SURFACE-001..004 COMPLETE via `rtmx verify`; `rtmx health` HEALTHY; `make ci`
green. `aegis run <prompt>` produces a transcript from a real `opencode run`
against the local model; `aegis loop` retains today's drain behavior + flags.
README + usage document the `run`→`loop` rename.
