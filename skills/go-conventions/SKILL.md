---
name: go-conventions
description: aegis-cli's Go style — one static air-gappable binary, std-lib-first, vendored deps, no CGO unless a serving probe needs it, and no telemetry ever. Use this whenever you write or review Go in this repo: choosing a dependency, structuring a package, deciding whether logic belongs here at all. Trigger it before reaching for a third-party library, before adding a build tag, and any time a task starts to feel like harness logic.
---

# go-conventions

aegis-cli is a single static binary that matches the rtmx project's conventions — Go,
CSV-in-git, no surprises. The style here is in service of the three non-negotiables
(`CLAUDE.md` §1): closed by construction, thin orchestrator, one requirement at a time.
Conventions that protect those win over personal taste.

## The rules

- **Single static binary.** Built offline from vendored deps:
  `GOFLAGS=-mod=vendor go build ./cmd/aegis`. It must drop onto an air-gapped host and run
  with no runtime fetch.
- **No CGO unless a serving probe genuinely needs it.** Default to pure Go for a clean
  static link and easy cross-target builds (`linux-cpu`, `darwin-metal`). CGO is a
  deliberate exception with a reason, not a default.
- **No telemetry, analytics, or phone-home — ever, by construction.** Not gated behind a
  flag; absent. See `airgap-hygiene` for the proof obligation.
- **Std-lib-first, deps vendored.** Reach for the standard library before a dependency.
  Anything you do add goes in `vendor/` and must build with the network disabled. A dep
  that phones home or pulls CGO is disqualified on sight.
- **Match rtmx.** Same Go idioms, same package layout instincts, same plainness. Two
  binaries that read like one project.

## Thin orchestrator — does this even belong here?

The sharpest convention is *what not to write*. aegis-cli owns only the loop, retry/
escalation policy, audit logging, metrics, and config (`internal/loop`, `audit`, `metrics`,
`config`). If a change feels like **harness logic — tool-calling, file editing, sandboxing,
prompt construction** — it does not belong here; it belongs to opencode/Goose behind
`internal/harness`. The previous Rust attempt failed by becoming the harness. Don't rebuild
it. When unsure, ask "is this orchestration or is this the agent's job?" — if the latter,
delegate it across the adapter boundary.

## Package shape

Keep loop logic free of load-bearing assumptions about *which* serving layer or harness is
behind it — both are swappable behind interfaces (`internal/serving`, `internal/harness`).
Config, not hard-coded vendor knowledge, drives the difference.

## Example

A requirement needs the harness driven headless. Tempting move: write tool-call parsing and
file-edit application in `internal/loop`. Wrong altitude — that's harness work. Right move:
extend the `internal/harness` adapter interface, keep the loop calling `Drive()` and reading
back a diff, std-lib only, no new dep. The binary still builds static and offline, still
reads like rtmx, still owns nothing it shouldn't.
