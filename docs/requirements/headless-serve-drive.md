# Requirement Specification — Headless Agent Drive via OpenCode `serve`

**Thread:** `BENCH-006..007` + `OC-009..010` · **Phase 7–8 / sprint v1.0** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `airgap-hygiene`, `build-to-spec`, `metrics-eval`
**Supersedes the drive mechanism of:** `REQ-BENCH-001` (its classic `opencode run` drive
wedged offline; the headless run is now delivered via the serve drive below — BENCH-001 closed)

## 1. Purpose

Make `aegis run` actually drive the local model to complete a task headlessly. This
spec records a hard-won discovery (2026-06-26): the original headless mechanism —
classic `opencode run --format json` (`REQ-BENCH-001`) — **does not work offline**,
and the `opencode serve` HTTP path that was thought "blocked upstream" **does work**
once two air-gap bootstrap egress vectors are closed and the *correct* route is used.

This was proven end-to-end: `opencode serve` (self-built v1.17.9) drove
`gemma4-qat:32k` to edit a file and make its test pass — real tool calls
(`glob`→`read`→`edit`), per-message token usage captured, **zero non-loopback egress**.

## 2. What we discovered (the corrected contract)

### 2.1 The route — `/session`, not `/api/session`

OpenCode's self-built binary exposes two overlapping session API surfaces mid-migration:

| Surface | Route | Behavior |
|---|---|---|
| **v2** (openapi-documented) | `POST /api/session/{id}/prompt` | **Queues only** — returns `admittedSeq`/`delivery`, never runs. `POST /api/session/{id}/wait` is a literal empty stub (`v2/session.ts:330`). |
| **v1** (the real executor) | `POST /session/{id}/message` | **Synchronous** — runs the agent turn against the model and returns the assistant message. The SDK `client.session.prompt` → this route (`sdk.gen.ts:615`, `root="/session"`). |

Unmatched routes fall through to the **web UI catch-all (HTML 200)** — which is why the
earlier probe (POSTing to `/api/session/.../prompt`, then reading an empty transcript)
read as "admitted but never executes." It was the wrong route, not an upstream gap.

**Working flow (loopback-only, no auth by default):**
1. `POST /session` — `{agent:"build", model:{providerID,id}, location:{directory}}` → session id
2. `POST /session/{id}/message` — `{parts:[{type:"text",text:…}], model:{providerID,modelID}, agent:"build"}` — blocks until the turn completes
3. `GET /session/{id}/message` — transcript with per-message `tokens` (input/output/reasoning)

### 2.2 Two bootstrap egress vectors that wedge the run offline

Before *any* run — `serve` or classic — OpenCode's bootstrap reaches for the network and
hangs offline (this is what made classic `opencode run` look broken, and it breaks `serve`
too):

- **ripgrep auto-download.** If `which("rg")` finds no real `rg` binary on PATH, OpenCode
  fetches ripgrep 15.1.0 from `github.com/BurntSushi/ripgrep` (`file/ripgrep.ts`). A shell
  function/alias `rg` (e.g. a wrapper) is invisible to its `which`. → must stage a real `rg`.
- **plugin npm install.** Bootstrap runs `npm install @opencode-ai/plugin@<self-version>`
  against `registry.npmjs.org` (`config/config.ts`, forkDetach + `Fiber.join`). The pinned
  self-built version exists on no registry; offline the TCP connect hangs (~70s even to a
  dead loopback registry) and the joined fiber stalls the run. → must pre-seed it offline.

Both are genuine **air-gap (GUARD) violations** independent of the drive mechanism, and both
must be closed for the closed-enclave target. They roll up under `REQ-ENCLAVE-001`
(whole-process-group EGRESS=0).

## 3. Requirements

### REQ-OC-009 — Stage ripgrep (no github fetch)
**The bundled OpenCode shall** resolve a *staged* ripgrep binary at launch (on the launch
PATH or `Global.Path.bin`) so its `which("rg")` succeeds and it never fetches ripgrep from
github at bootstrap. *Target:* a real `rg` is staged in/alongside `deploy/opencode`; a run
makes zero `github.com/BurntSushi/ripgrep` requests under the egress gate. *Test:*
`internal/opencode::TestRipgrepStaged`. *Depends on:* `REQ-OC-004`.

### REQ-OC-010 — Pre-seed the plugin dependency offline (no npm egress)
**The bundled OpenCode shall** find its `@opencode-ai/plugin` dependency already satisfied
so bootstrap performs no npm install and issues no registry request. *Target:* the staged
opencode config dir ships a satisfying `node_modules` + `package-lock.json` for
`@opencode-ai/plugin` (or the plugin auto-install is disabled); bootstrap makes no
`registry.npmjs.org` request and does not stall on the install; EGRESS=0. *Test:*
`internal/opencode::TestPluginInstallSuppressed`. *Depends on:* `REQ-OC-004`.

### REQ-BENCH-006 — Drive the turn via `opencode serve` (synchronous executor)
**aegis shall** drive a headless agent turn through `opencode serve`'s synchronous executor:
create a session (`POST /session`), post the prompt (`POST /session/{id}/message` with a
`{parts, model, agent}` body, blocking until the turn completes), and read the transcript
(`GET /session/{id}/message`) — using the `/session` base (no `/api` prefix) and **not**
`/wait`. Classic `opencode run` is abandoned for the drive (it wedges offline). All traffic
is loopback-only. *Test:* `internal/opencode::TestServeDriveSynchronous`. *Depends on:*
`REQ-OC-006`, `REQ-OC-009`, `REQ-OC-010`.

### REQ-BENCH-007 — Validate the drive against the REAL binary (not a mock)
**aegis shall** validate the serve *drive* against the *real* self-built OpenCode binary: a
gated integration test starts `opencode serve`, drives a trivial edit task on a local model,
and asserts a real transcript with per-message token usage — exercising the actual routes and
response shapes a mock can silently get wrong. (When first run it caught two such bugs the
unit mock had encoded: the flat session-create response `{"id":…}` and the flat message-list
array, neither wrapped in `{"data":…}`.) Whether the model lands the edit and the task tests
pass is *model capability* — `REQ-RUNQ-004`, gated on the `SERVE-016` bake-off — so that
outcome is logged, not asserted here. *Test:* `test::TestServeDriveRealBinary` (release/
integration tier, gated on `AEGIS_REAL_ENDPOINT` + `AEGIS_REAL_MODEL` + a resolvable binary +
ripgrep). *Depends on:* `REQ-BENCH-006`.

## 4. Design notes & risks

- **`internal/opencode/serve.go` already exists but is wrong.** It posts to `/api/session`
  (create) and `/api/session/{id}/prompt` with `{prompt:{text}}` (the queue route — never
  executes), and expects `/wait`. The fix is route + body, not new architecture: drive OpenCode,
  don't rebuild it. The existing unit test mocked the wrong paths, so add the real-binary
  integration test (BENCH-007) rather than trusting the mock.
- **Why classic `opencode run` is abandoned, not fixed.** It is itself an HTTP client of an
  internally-spawned server hitting the same v1/v2 surface; driving `serve` directly is simpler,
  observable (we own the session lifecycle + budget), and is the path proven to work.
- **Synchronous blocking + budget.** `POST /session/{id}/message` blocks for the whole turn;
  on a CPU-bound local model that is minutes. The `REQ-RUNQ-001` wall-clock budget still applies
  (abort + partial transcript); a small local model needs a generous default.
- **Reasoning models.** `gemma4-qat:32k` is a reasoning model; it nonetheless emitted valid
  tool calls under the `build` agent — a positive data point for `REQ-RUNQ-002`, though the
  agent/system config that *reliably* elicits tool calls across candidate models stays open
  (and the model itself is still subject to the `REQ-SERVE-016` bake-off).
- **Pin coupling.** Routes/stubs are specific to the pinned OpenCode (v1.17.9). An `OC-008`
  pin bump must re-validate BENCH-006/007 — the v1/v2 surface is migrating upstream.

## 5. Exit criteria

OC-009, OC-010, BENCH-006 COMPLETE via `rtmx verify`; BENCH-007 green against the real binary
under the egress gate (EGRESS=0). `aegis run` completes the trivial edit task end-to-end on a
local model with a populated usage transcript, unblocking `REQ-RUNQ-004` (real-task completion)
and the intent-bench profiling run (`BENCH-001..005`).
