# Requirement Specification — aegis TUI experience: rtmx intent UI + in-TUI model provisioning (OC-019..022)

**Thread:** `OC-019..022` · **Builds on:** `OC-014` (rebranded TUI), `OC-017` (patch set),
`OC-006` (hardened config), `RTMX-*` (intent layer), the model catalog (`SERVE`/`MODEL`).
Status: PLANNED.

## 1. Why

The aegis TUI (the rebranded OpenCode, OC-014) is the operator's **entire interface** in a
closed enclave — there is no friendly shell, no `pip`, no `curl`, often no second window. Two
things that currently live in the shell should live *inside* the TUI:

1. **The intent layer (rtmx)** — aegis's reason for being. It should be visible (traceability +
   state) and driveable (commands) from the TUI, not a separate terminal.
2. **Model provisioning** — the operator must be able to stage the offline local model from the
   TUI itself: browse, download/stage, verify, calibrate, select — seamless and observable.

## 2. Requirements

### REQ-OC-019 — rtmx bundled + auto-configured in the TUI
**The aegis distribution shall** bundle the `rtmx` binary (in `libexec`, like opencode/llama,
REL-006) and ship the TUI pre-configured with the rtmx MCP server, so rtmx tools
(`next`/`claim`/`verify`/`set_status`) are available in the TUI **out of the box** — no operator
setup. *Target:* a fresh `aegis` launch resolves `rtmx mcp-server` on the launch PATH and the
rtmx MCP is live in the session. *Test:* `test::TestRtmxBundledConfigured`. *Depends on:*
`REQ-OC-006`, `REQ-REL-006`.

### REQ-OC-020 — `/rtmx` slash command
**The TUI shall** expose an `/rtmx <subcommand>` command that runs the rtmx commands
(`next`, `claim`, `verify`, `status`, `health`, `backlog`) and renders the output inline — the
operator drives the intent loop without leaving the TUI. *Target:* `/rtmx status` and
`/rtmx next` work in-session and show results. *Test:* `test::TestRtmxSlashCommand`. *Depends
on:* `REQ-OC-019`, `REQ-OC-017`.

### REQ-OC-021 — Requirements traceability UI
**The TUI shall** render an rtmx traceability view — requirements, status
(COMPLETE/PARTIAL/MISSING), completion %, and the requirement→test matrix — refreshed as the
loop closes requirements, so intent state is **observable while coding**. *Target:* a panel/view
shows the live RTM + progress; it updates when `rtmx verify` closes a requirement. *Test:*
`test::TestTraceabilityUI`. *Depends on:* `REQ-OC-019`, `REQ-OC-017`.

### REQ-OC-022 — In-TUI model provisioning (seamless + observable)
**The TUI shall** let the operator provision the offline local model end-to-end without a shell:
browse the **RAM-aware catalog** (`deploy/models/catalog.json`, striking models that won't fit),
stage/download a model with **live progress**, **verify its digest**, calibrate, and select it —
then see live model state (loaded model, health, tok/s). *Target:* from a fresh TUI the operator
selects + stages a catalog model and is coding against it, with visible progress + verification,
never touching `setup.sh`. *Test:* `test::TestInTuiModelProvision`. *Depends on:* `REQ-OC-014`,
`REQ-OC-017`.

## 3. Design notes — how (per the "consider how" ask)

- **Surface.** Two implementation surfaces, both already in our control: (a) **OpenCode
  plugins/commands** — we already seed `@opencode-ai/plugin` (OC-010), so `/rtmx` and the
  provisioning flow can be plugin commands; (b) **TUI patches** via the OC-017 patch set for the
  traceability panel + the model picker's provisioning entry. The TUI is the *front end*; the
  bundled **aegis binary is the engine** — `/rtmx` shells the bundled `rtmx`, and provisioning
  drives aegis's existing `stage-model` / `serve` / catalog flow. No new engines.
- **Observable.** OpenCode already streams session events; provisioning + the loop emit progress
  events (download %, digest OK, calibration, tok/s) the TUI renders — the same event channel
  the agent uses, so it's live without polling.
- **Air-gap.** Everything is loopback/local: rtmx (stdio MCP), the RTM (in-repo CSV), the model
  (side-loaded GGUF / catalog). Provisioning "download" means staging a **side-loaded** GGUF or
  a locally-mirrored catalog source — never public egress (GUARD-001 still holds).
- **Sequencing.** `OC-019` (bundle + config) is the floor. `OC-020` (`/rtmx`) and `OC-021`
  (traceability UI) ride the rtmx MCP + the patch set. `OC-022` (provisioning) is the largest —
  a TUI flow over the existing catalog/stage/serve engine.

## 4. Notes

- This is the **operator UX** half of "aegis-ify OpenCode" (OC-012..018 is the hardening half).
  Together they make the bundled TUI *the* aegis product, not a configured OpenCode.
- `OC-021`/`OC-022` are UX-heavy; expect them to decompose (PROPOSE) into atomic children when
  claimed.

### REQ-OC-023 — Auto model orchestration (bare `aegis` just works)

**The bare `aegis` command shall** bring up a usable model before opening the TUI, so the operator
never faces an empty UI thrashing on "Cannot connect to API":

1. **Preflight** the model endpoint; if a model already answers on loopback, open the TUI against it.
2. **Auto-serve.** Otherwise resolve a host calibration (`$AEGIS_CALIBRATION`, `~/.config/aegis`,
   repo default) + its model GGUF, launch `llama-server` in the background with visible load
   progress, wait for readiness, then open the TUI — labelling the picker with the *actual* served
   model id, never the `local-moe` placeholder.
3. **Resource-aware.** The model served is the one chosen at provision time by the resource-aware
   plan (`internal/install.Plan` picks the largest envelope the host can hold by RAM: 26B-A4B
   `< 24 GiB`, 35B-A3B `< 56 GiB`, larger `≥ 56 GiB`). **Not every model fits every system.**
4. **Guide, never thrash.** If no model can be brought up, print resource-aware provisioning
   guidance naming the host's best-fit tier + how to download (connected host) or source a local
   GGUF, then calibrate — and exit, rather than open an unusable UI. The air-gap rule holds: a model
   is only ever downloaded explicitly on a connected host, never silently or in the enclave.

**Verify:** `cmd/aegis::TestModelAutoServeResourceAware`. **Deps:** SERVE-004, MODEL-004, OC-019.

**Fast-start guarantee (OC-023 refinement).** The launch never blocks on a full bench. A side-loaded
model with no calibration is started immediately with a synthesized, host-shaped *seed* calibration
(`internal/install.Plan`: threads ≈ physical cores, `-ngl` per target), persisted to
`~/.config/aegis/calibration.json` so the background profiler / `bench.sh` refine the same file in
place. The picker prefers the smallest GGUF (fastest to load). Time-to-first-token beats optimality;
the rigorous resource-fit analysis runs *behind* the operator (the profiler), never in front.
