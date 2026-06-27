# Requirement Specification — intent-bench Profiling

**Thread:** `BENCH-001..005` · **Phase 8 / sprint v0.4** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `metrics-eval`, `airgap-hygiene`

## 1. Purpose

Profile the **local-only aegis stack** (OpenCode + a loopback local model + the
rtmx intent layer) against **hosted Claude Sonnet 4** on
[`intent-bench`](https://github.com/intent-bench/intent-bench), to quantify how a
local-only system with an integrated intent layer performs vs. a frontier hosted
model. intent-bench is the same intent-bench methodology aegis already mirrors in
its own CI (ACR ≈ intent-bench's "completion rate").

## 2. The intent-bench contract (as it actually is)

intent-bench is a **bash A/B harness**, not an HTTP/Python framework. A
system-under-test is a wrapper script:

```
agents/<name>.sh <workdir> <model> <prompt_file> <result_dir> <max_budget>
```

It must `cd $workdir`, drive the agent autonomously, and write
`$result_dir/transcript.jsonl` (Claude Code **stream-json** or **NDJSON** with
per-message `usage`; a final `{"type":"result","usage":{...},"num_turns":N}`
record gives authoritative totals) + `stderr.log`. Scoring (`lib/verify.sh`) runs
the experiment's `test_command` in the workdir: exit 0 ⇒ **PASS**. North-star =
**completion rate** (PASS fraction) + tokens; secondary = knowledge-entropy,
backtrack rate. The Sonnet-4 baseline is `agents/claude-code.sh`
(`claude --model … -p … --output-format stream-json`). `treatments/rtmx.sh`
already seeds `.intent-bench/` + an MCP `.mcp.json` + runs `rtmx install` — **rtmx
is a first-class treatment in intent-bench**, which is ideal for us.

## 3. The headless surface (no OpenCode PR needed)

OpenCode exposes a full HTTP API via `opencode serve` —
`packages/sdk/openapi.json`, 151 paths, including: `POST /session` (create),
`POST /session/{id}/prompt` (run a turn), `POST /session/{id}/wait`,
`GET /session/{id}/message` (transcript + usage), `GET /session/{id}/diff`,
`POST /session/{id}/abort`. So the headless agent-run is: **serve → create
session → prompt → wait → collect messages/usage**, all on loopback. We do **not**
need to add a completion surface to OpenCode or open an upstream PR.

## 4. Requirements

### REQ-BENCH-001 — Headless agent-run (`aegis solve`)
**aegis shall** provide a headless mode that, given a prompt + workdir, starts
`opencode serve` (loopback), creates a session rooted at the workdir, posts the
prompt, waits for autonomous completion against the configured local model, and
honors a budget/timeout — zero non-loopback egress. *Test:*
`internal/opencode::TestServeClientDrive`. *Depends on:* REQ-OC-006.

### REQ-BENCH-002 — Transcript export
**The run shall** emit `transcript.jsonl` in intent-bench's format — NDJSON with
per-message `usage` (input/output/cache tokens) and `tool_use` parts, plus a final
`result` record — parseable by `lib/parse_transcript.py`. *Test:*
`internal/bench::TestTranscriptExport`. *Depends on:* REQ-BENCH-001.

### REQ-BENCH-003 — intent-bench adapter
**aegis shall** ship `scripts/intent-bench/aegis.sh` conforming to the SUT
contract (`<workdir> <model> <prompt_file> <result_dir> <max_budget>`), driving
`aegis solve` and writing `transcript.jsonl` + `stderr.log`. *Test:*
`test::TestBenchAdapterContract`. *Depends on:* REQ-BENCH-002.

### REQ-BENCH-004 — rtmx as the OpenCode intent treatment
**The treatment run shall** wire rtmx into OpenCode's MCP discovery (the seeded
`.mcp.json` / OpenCode mcp config) so intent-tool calls are attributed;
`INTENT_TOOL_PREFIX` shall match OpenCode's rtmx tool names, and control runs shall
report zero intent-tool tokens (intent-bench ledger rule). *Test:*
`test::TestBenchIntentTreatment`. *Depends on:* REQ-BENCH-003.

### REQ-BENCH-005 — Profiling runbook
**aegis shall** document the procedure (`docs/intent-bench-profiling.md`): the
exact `bench.sh run … --agent aegis` invocations for control + rtmx treatment vs.
the `claude-code`/sonnet-4 baseline, and how to read completion-rate / tokens /
entropy. *Test:* `test::TestBenchRunbook`. *Depends on:* REQ-BENCH-003.

## 5. Design notes & risks

- **`aegis solve` is a new, focused capability** distinct from `aegis run` (the
  rtmx-drain loop): one-shot prompt → autonomous OpenCode session → transcript. It
  is an HTTP client of OpenCode's serve API (a small `internal/opencode` serve
  client), so it stays "drive OpenCode, don't rebuild it."
- **Token accounting:** a local model is free, so the USD budget is informational;
  we still emit token counts (from OpenCode's message `usage`) for the
  efficiency/entropy metrics. If OpenCode's usage fields differ from Claude's, the
  exporter maps them.
- **Apples-to-apples caveat:** the baseline drives Claude Code (a different
  harness) on Sonnet-4; we drive OpenCode on a local model. The cleanest
  comparison also runs **OpenCode-on-Sonnet-4** as a second baseline to isolate
  model vs. harness — worth adding once the adapter works.
- This is **out-of-enclave profiling** (it talks to a local model, but the harness
  + git repo are public benchmarks); it does not touch controlled data.

## 6. Exit criteria

BENCH-001..005 COMPLETE via `rtmx verify`; a real profiling run produces a
populated `results/summary.csv` comparing local-aegis (control + rtmx treatment)
to the Sonnet-4 baseline on at least one experiment (e.g. `url-shortener`). The
**full-suite** headline run is its own requirement — see §7 / `REQ-BENCH-009`.

## 7. The headline run (full suite)

### REQ-BENCH-009 — Execute the full intent-bench suite
**aegis shall** execute a real intent-bench profiling run across the **full** experiment suite —
every experiment, for aegis *control* and the *rtmx treatment*, against the `claude-code` /
Sonnet-4 baseline — and record a populated `results/summary.csv` plus the completion-rate /
tokens / knowledge-entropy comparison. This is the headline number the BENCH thread exists to
produce; §6 above covered only one experiment. The run drives `aegis run`, so it is gated on the
serve-drive wiring (`REQ-BENCH-008`) and on a decided model (`REQ-SERVE-016`) — without the model
pick the numbers are noise; `REQ-RUNQ-002` (reliable tool-calling) materially lifts completion
rate. *Test:* `test::TestIntentBenchSuiteRun` — gated (needs an intent-bench checkout +
`AEGIS_REAL_ENDPOINT`/`AEGIS_REAL_MODEL`; skipped in CI), asserts a populated `results/summary.csv`
covering every experiment for control + treatment. *Depends on:* `REQ-BENCH-008`, `REQ-SERVE-016`.

**Proposed decomposition (machine-authored, awaiting human approval — `PROPOSED`, not claimable):**
A decomposition pass split BENCH-009 into two atomic children that separate the buildable-now
capability from the model-gated run:
- `REQ-BENCH-009-P01` — the full-suite *runner* (drives the adapter over every experiment ×
  control/treatment/baseline); buildable once `REQ-BENCH-008` lands.
- `REQ-BENCH-009-P02` — the real *run + recorded headline* (`results/summary.csv` + the
  Fisher / Mann-Whitney comparison under `eval/intent-bench/`); gated on `REQ-SERVE-016` + an
  intent-bench checkout.
Both inherit the parent test (`TestIntentBenchSuiteRun`); a narrower test for each is flagged
for the human to author on approval. `REQ-BENCH-008` is atomic — the same pass found no split.

## Addendum — upstream headless-run gap (validated against v1.17.9)

Reverse-engineered + tested the real serve API end to end against the self-built
OpenCode v1.17.9: routes under `/api`, readiness `GET /openapi.json`, HTTP Basic
auth with the password OpenCode generates into `Global.Path.state/password`
(username `opencode`). **Working:** create session (`POST /api/session` with
`agent:"build"`, model, location), prompt admission (`POST /api/session/{id}/prompt`
→ 200). **Blocked upstream:** the admitted prompt does **not** run through the
public HTTP surface — `GET /api/session/{id}/message` never populates, and
`POST /api/session/{id}/wait` returns `503 "Session wait is not available yet"`
(an explicit stub). Attaching the SSE `/api/event` stream did not start the run
either.

So **REQ-BENCH-001 is blocked on an OpenCode 2.0-preview gap**, not on aegis. The
serve client (`internal/opencode/serve.go`) is built to the correct contract and
unit-tested; `internal/bench` transcript export is done. Options (decision
pending): (a) file an upstream issue/PR to implement `/wait` + headless prompt
execution; (b) pin an OpenCode release whose headless surface works (classic v1
had a one-shot `opencode run <prompt>`), trading off the "latest stable" policy;
(c) revisit when upstream lands it. The client + transcript + adapter scaffolding
are ready to light up the moment the run executes.

### Addendum 2 — RESOLVED 2026-06-26: the serve path works (wrong route, not a gap)

The "blocked upstream" verdict above was **wrong**. Re-tested against the same
self-built v1.17.9 with the bootstrap egress vectors closed (staged `rg`, pre-seeded
plugin) and the **correct route**, `opencode serve` drove `gemma4-qat:32k` end-to-end
(file edited, pytest PASS, real tool calls, per-message usage, EGRESS=0). The original
probe hit `/api/session/{id}/prompt` — the **v2 queue route** that only admits (its
`/wait` is an empty stub) — and unmatched POSTs fall through to the web UI (HTML 200),
reading as "admitted but never runs." The real synchronous executor is **`POST
/session/{id}/message`** (base `/session`, no `/api`). `serve.go` uses the wrong
paths/body and its unit mock encoded them, hiding the bug. The corrected drive +
bootstrap hardening + a real-binary integration test are specified in
**`docs/requirements/headless-serve-drive.md`** (`REQ-BENCH-006..007`, `REQ-OC-009..010`),
all delivered. `REQ-BENCH-001`'s headless run is now satisfied via that serve drive (its
classic-`opencode run` mechanism wedged offline and was superseded).
