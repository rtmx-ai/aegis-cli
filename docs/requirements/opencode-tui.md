# Requirement Specification — OpenCode TUI as the Centerpiece

**Thread:** `TUI-001..006` · **Phase 7 / sprint v0.3** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `airgap-hygiene`, `rtmx-loop`, `go-conventions`

## 1. Purpose & scope

aegis-cli's centerpiece is the **OpenCode TUI** (MIT) — a top-tier agentic coding
experience — driven by a **local model** (Ollama / llama.cpp, loopback) with
**rtmx as the intent layer**. Running `aegis` launches that TUI inside the closed
enclave. aegis **bundles and launches OpenCode; it does not fork or rebuild it**
(CLAUDE.md §1). This thread delivers the launch path, the model + rtmx wiring, the
air-gap hardening of the launch, offline bundling, and graceful failure.

In scope: binary resolution, the launch command/config, loopback-model + rtmx-MCP
wiring, air-gap-hardened OpenCode config, release bundling, missing-binary
guidance. Out of scope: modifying OpenCode's internals (we configure/drive it),
and the headless `aegis run` loop (already shipped — it remains for unattended
drains; the TUI is the interactive front-end over the same rtmx intent layer).

## 2. Definitions

- **Launch** — aegis resolves the OpenCode binary and execs it with the hardened
  config + local-model + rtmx-MCP wiring, handing the terminal to OpenCode's TUI.
- **Bundled** — the OpenCode binary ships in the aegis release artifacts so a
  closed host needs no separate fetch (stage-then-disconnect).
- **Hardened config** — `deploy/opencode/opencode.json`: offline, telemetry/share/
  autoupdate off, model = loopback, rtmx registered as a stdio MCP server.

## 3. Requirements

### REQ-TUI-001 — `aegis` launches the OpenCode TUI
**Running `aegis` with no subcommand shall** resolve the bundled OpenCode binary
and exec it (handing over the terminal); existing subcommands (`run`, `init`,
`verify-env`, `frame`, …) still dispatch, and `aegis --help` shows usage.
*Rationale:* the TUI is the centerpiece experience. *Acceptance:* binary
resolution finds opencode on PATH or alongside the aegis executable; the
constructed launch targets that binary. *Test:*
`internal/opencode::TestResolveAndCommand`. *Depends on:* REQ-HARNESS-001.

### REQ-TUI-002 — Local loopback model
**The launched OpenCode shall** be pointed at the configured local loopback model
endpoint (Ollama/llama.cpp), with no remote provider. *Test:*
`internal/opencode::TestLaunchUsesLoopbackModel`. *Depends on:* TUI-001.

### REQ-TUI-003 — rtmx as the MCP intent layer
**The launch shall** register the rtmx stdio MCP server with OpenCode so the
operator drives work through rtmx intent (next/claim/verify/set_status) inside
the TUI. *Test:* `internal/opencode::TestLaunchWiresRtmxMCP`. *Depends on:* TUI-001.

### REQ-TUI-004 — Air-gap-hardened launch
**OpenCode shall** launch under the hardened config — offline, telemetry/share/
autoupdate off, no model-registry fetch — so egress stays loopback-only.
*Test:* `internal/opencode::TestLaunchIsHardened`. *Depends on:* TUI-001, REQ-GUARD-002.

### REQ-TUI-005 — Bundled for offline distribution
**The release shall** bundle the OpenCode binary alongside aegis (covered by the
checksums manifest) so a closed host installs both from one artifact set.
*Test:* `test::TestReleaseBundlesOpenCode`. *Depends on:* REQ-BUILD-002.

### REQ-TUI-006 — Graceful missing-binary handling
**When the OpenCode binary is absent, aegis shall** print clear guidance on
staging/bundling it and exit non-zero — never a panic or opaque error. *Test:*
`internal/opencode::TestMissingBinaryGuidance`. *Depends on:* TUI-001.

## 4. Design constraints

- Bundle, don't rebuild (CLAUDE.md §1): aegis execs the OpenCode binary; it does
  not reimplement the harness. The launch is config-driven via the hardened
  `deploy/opencode/opencode.json`.
- Single static aegis binary (Go, std-lib); OpenCode is a separate bundled binary
  execed as a child — keeping each process's egress independently auditable.
- The real OpenCode invocation (exact flags/config path) must be validated against
  a staged OpenCode build, like real-model validation — the launch-command
  construction + config content are unit-tested; the live TUI launch is gated.

## 5. Verification & exit criteria

All six COMPLETE via `rtmx verify`, `rtmx health` HEALTHY, `make ci` green. Build
order: TUI-001 → 002/003/004/006, then TUI-005. Live-TUI validation against a
staged OpenCode is a gated manual step (see docs/model-validation.md for the
pattern).
