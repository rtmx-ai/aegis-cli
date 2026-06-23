# Requirement Specification — Live Run

**Thread:** `RTMX-004..006`, `RUN-001..005`, `FEAT-011..012` · **Phase 4 / sprint v0.4** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `rtmx-loop`, `build-to-spec`, `airgap-hygiene`, `unattended-operation`

## 1. Purpose & scope

Today `aegis run` (`cmd/aegis/main.go::cmdRun`) does only the safe prelude: it
loads and validates config, selects the harness adapter (`selectHarness`), and
prints a one-line plan. It never connects a real rtmx client, never runs a
serving preflight, never claims a requirement, and never drives the loop
(`internal/loop`). The loop and the built-in harness (`internal/harness/serving`)
already exist and are tested against fakes — but nothing wires them to a live
backlog. The only `rtmx.Client` that exists is the in-memory `Fake`.

This thread makes `aegis run` **actually drain a real backlog**. It supplies a
real `rtmx.Client` that speaks MCP over stdio to `rtmx mcp-server --stdio`, with
a CLI-subcommand fallback when MCP is unavailable; it adds a serving health
preflight that aborts cleanly before any claim; it writes the append-only audit
log to `AuditPath` and prints an accurate run summary; it reuses the verify-env
gate so a non-closed environment refuses to start; and it honors the run budget,
circuit breaker, and park-on-escalation against the live client. After this
thread, `aegis run --once` and `aegis run` (drain) work a real fixture backlog
end to end, and the process exit code reflects the outcome.

In scope: the real rtmx client (MCP stdio + CLI fallback), lifecycle/status
mapping and atomic claim, the `cmdRun` wiring (preflight, gate, loop construction,
audit-to-file, summary), and the two E2E drain scenarios. Out of scope: the loop
contract itself (already built — `internal/loop`), the harness internals (the
`harness-serving` thread), serving calibration/launch, and any non-loopback
network path (forbidden by construction).

## 2. Definitions

- **Real client** — a production `rtmx.Client` implementation talking to the
  actual `rtmx` binary, as opposed to the in-memory `Fake` used by unit tests.
- **MCP stdio transport** — JSON-RPC framed over the stdin/stdout of a
  long-lived `rtmx mcp-server --stdio` child process, exposing
  `next`/`claim`/`release`/`verify`/`status`/`health` as MCP tools.
- **CLI fallback** — the same `Client` interface satisfied by shelling discrete
  `rtmx next|claim|release|verify|status|health` subcommands, used when the MCP
  server cannot be started.
- **Preflight** — a serving health probe run once at the start of `aegis run`,
  before the first claim, that must succeed for the run to proceed.
- **Fixture DB** — a temporary `.rtmx` directory seeded with a known backlog,
  used to exercise the real client and the live drain hermetically.

## 3. Requirements

Each requirement is well-formed (EARS style), independently verifiable, and
linked to the acceptance test that closes it via `rtmx verify`.

### REQ-RTMX-004 — Real rtmx client over MCP stdio
**The real rtmx client shall** speak MCP over stdio to `rtmx mcp-server --stdio`,
implementing the full `rtmx.Client` interface
(`Next`/`Claim`/`Release`/`Verify`/`WriteStatus`/`Health`) as JSON-RPC tool calls
over the child process's stdin/stdout.
*Rationale:* the loop depends only on `rtmx.Client`; this is the real
implementation behind that seam, and MCP stdio is the primary transport rtmx
exposes. *Acceptance:* against a temp `.rtmx` fixture DB, the client launches
`rtmx mcp-server --stdio`, and every interface method round-trips —
`Next` returns the seeded requirement, `Claim`/`Release` succeed, `Verify`
returns a result, `WriteStatus` persists, and `Health` reports reachable — making
no non-loopback call. *Test:* `internal/rtmx::TestMCPClientRoundTrip`.
*Depends on:* REQ-RTMX-001.

### REQ-RTMX-005 — CLI fallback client
**When** the MCP server is unavailable, **the rtmx client shall** satisfy the
same `rtmx.Client` interface by shelling discrete `rtmx` subcommands
(`next`/`claim`/`release`/`verify`/`status`/`health`) against the database.
*Rationale:* MCP stdio is preferred but the CLI path is the dependable fallback;
the loop must not know or care which transport is active. *Acceptance:* with MCP
disabled/unavailable, the CLI client drives a temp DB through the full interface
with the same observable results as the MCP client, returning typed errors rather
than panicking on a failed subcommand. *Test:*
`internal/rtmx::TestCLIClientFallback`. *Depends on:* REQ-RTMX-004.

### REQ-RTMX-006 — Lifecycle statuses + atomic claim, faithful verify
**The rtmx client shall** honor requirement lifecycle statuses — skipping
`closed`, `blocked`, and `proposed` requirements in `Next` — perform claim and
release atomically so a requirement cannot be double-claimed, and map a `verify`
result faithfully to `(bool, error)`.
*Rationale:* correctness of the live loop hinges on the client never handing out
unclaimable work, never double-claiming, and reporting verify exactly as rtmx
sees it. *Acceptance:* `Next` never returns a `closed`/`blocked`/`proposed`
requirement; a second `Claim` of an already-claimed id is refused; a passing
verify maps to `(true, nil)` and a failing verify to `(false, nil)`, with a
transport/engine error mapped to a non-nil error. *Test:*
`internal/rtmx::TestClientStatusMapping`. *Depends on:* REQ-RTMX-004.

### REQ-RUN-001 — `aegis run` executes the live loop
**`aegis run` shall** execute the live control loop — real rtmx client + built-in
serving-backed harness + audit-to-file — for both `--once` and the continuous
drain, and **shall** set a process exit code that reflects the run outcome.
*Rationale:* this is the whole point of the thread: turn the report-only `cmdRun`
into a real driver of `internal/loop` against a live backlog. *Acceptance:*
against a real fixture backlog, `aegis run --once` works exactly one requirement
and `aegis run` (drain) works requirements until the backlog is empty or a stop
condition fires; a clean drain exits 0 and an aborting/error condition exits
non-zero. *Test:* `cmd/aegis::TestRunLiveDrains`. *Depends on:* REQ-RTMX-004,
REQ-HARNESS-003, REQ-LOOP-005.

### REQ-RUN-002 — Serving health preflight before claiming
**Before claiming any work, `aegis run` shall** run a serving health preflight
against the configured endpoint, and **if** the endpoint is unreachable or
unhealthy **it shall** abort cleanly with a clear error and make no claim.
*Rationale:* claiming a requirement against a dead model strands the claim and
wastes the budget; failing fast before the first claim keeps the backlog clean
and the run resumable. *Acceptance:* with an unreachable/unhealthy endpoint,
`aegis run` exits non-zero with a clear message, the rtmx client's `Claim` is
never invoked, and no requirement changes status; with a healthy endpoint the run
proceeds to the loop. *Test:* `cmd/aegis::TestRunPreflightAbortsWhenUnhealthy`.
*Depends on:* REQ-SERVE-012, REQ-RUN-001.

### REQ-RUN-003 — Audit-to-file + run summary
**`aegis run` shall** open an append-only audit log at `AuditPath` for the run and
**shall** print a run summary reporting attempted, closed, parked, breaker-tripped,
and budget-exhausted.
*Rationale:* an unattended run must leave an immutable in-enclave record and a
human-readable account of what it did; both feed review and metrics. *Acceptance:*
after a drain the file at `AuditPath` contains append-only claim/verify/release
lines for the work performed, and stdout shows a summary whose counts match the
loop `Result` (`Attempted`/`Closed`/`Parked`/`BreakerTripped`/`BudgetExhausted`).
*Test:* `cmd/aegis::TestRunWritesAuditAndSummary`. *Depends on:* REQ-RUN-001,
REQ-AUDIT-001.

### REQ-RUN-004 — Refuse to start in a non-closed environment
**If** the environment is not closed — `AllowEgress` is set or the endpoint is
non-loopback — **`aegis run` shall** refuse to start, reusing the verify-env gate,
aborting before any claim.
*Rationale:* the air-gap non-negotiable expressed at the run entrypoint: a run
must never touch the backlog from an open environment. *Acceptance:* with
`AllowEgress=true` or a non-loopback endpoint, `aegis run` exits non-zero before
constructing the loop, makes no claim, and reports the closed-env failure; a
closed config proceeds. *Test:* `cmd/aegis::TestRunRefusesOpenEnv`. *Depends on:*
REQ-RUN-001, REQ-CLI-001.

### REQ-RUN-005 — Budget, breaker, and park honored live
**`aegis run` shall** honor the run budget (max requirements + wall-clock), the
circuit breaker (halt after M consecutive failures), and park-on-escalation
against the live rtmx client.
*Rationale:* unattended safety must hold against the real client, not just the
fake; the loop already implements these, so this requirement proves they survive
live wiring. *Acceptance:* with a fixture backlog, a configured `--max`/`--budget`
stops the run at the cap (summary shows budget exhausted), repeated verify
failures trip the breaker after M (summary shows breaker tripped), and a
requirement that exhausts retries is parked (marked blocked, logged, released) and
the drain continues. *Test:* `cmd/aegis::TestRunHonorsBudgetAndBreaker`.
*Depends on:* REQ-RUN-001, REQ-LOOP-007, REQ-LOOP-008.

### REQ-FEAT-011 — E2E: live drain closes all closeable requirements
**Scenario (Gherkin, `features/live_run.feature`):** `aegis run` drains a fixture
rtmx backlog with the built-in harness and a mock model → every closeable
requirement is closed by verify → the printed run summary is accurate. *Test:*
`test/bdd::TestFeatures`. *Depends on:* REQ-RUN-001.

### REQ-FEAT-012 — E2E: a failing requirement parks during a live drain
**Scenario (Gherkin, `features/live_run.feature`):** during a live drain one
requirement fails verify and is parked (marked blocked, logged, released) → the
run continues working the remaining backlog → the run summary reflects the park.
*Test:* `test/bdd::TestFeatures`. *Depends on:* REQ-RUN-005.

## 4. Design constraints

- Std-lib only in the shipped path (enforced by
  `test.TestRuntimeBinaryIsStdLibOnly`). The MCP stdio client is hand-rolled over
  `os/exec` + `encoding/json` — no third-party MCP library.
- The real client is testable hermetically: launch `rtmx mcp-server --stdio`
  against a temp `.rtmx` fixture for REQ-RTMX-004/006, and shell `rtmx` against a
  temp DB for the CLI fallback (REQ-RTMX-005). No network, no shared global DB.
- Generation and verification stay phase-separated; the loop
  (`internal/loop`) already enforces this and its contract is not changed by this
  thread — `cmdRun` only constructs `loop.Deps` and calls `Run`.
- The audit log is append-only and stays in-enclave: `aegis run` opens it via
  `audit.Open(cfg.AuditPath, ...)` (local file, `O_APPEND`), never the network.
- The closed-environment check is the existing verify-env gate
  (`config.Validate` + `AllowEgress`); REQ-RUN-004 reuses it rather than
  re-implementing a loopback check.

## 5. Verification & exit criteria

The thread is complete when all ten requirements are `COMPLETE` via
`rtmx verify --update`, `rtmx health` is HEALTHY at 100% coverage, and `make ci`
is green (race, lint, govulncheck, cover-gate ≥ floor, EGRESS=0, TRACE=100%,
ACR-regression). Build order follows the dependency graph: REQ-RTMX-004 first,
then REQ-RTMX-005 and REQ-RTMX-006; then REQ-RUN-001; then REQ-RUN-002,
REQ-RUN-003, REQ-RUN-004, and REQ-RUN-005; finally the E2E scenarios REQ-FEAT-011
and REQ-FEAT-012.
