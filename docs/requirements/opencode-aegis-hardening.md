# Requirement Specification — aegis-ify the bundled OpenCode (OC-012..018)

**Thread:** `OC-012..018` · **Extends:** `OC-001..011` (build/bundle/launch), `GUARD-001`
(egress=0), `MODEL-006/007` (origin governance). Status: PLANNED.

## 1. Why — the gap observed on `darwin-metal` (MBP M5)

Running `aegis` launches the bundled OpenCode TUI, and its model picker listed **all cloud
frontier models** (Anthropic, OpenAI, Google, …). For an air-gap / CUI / ITAR product this is
disqualifying: a selectable cloud model is an egress vector and a control violation, even if
never chosen.

**Root cause.** `OC-011` disabled OpenCode's *runtime* models.dev fetch
(`OPENCODE_DISABLE_MODELS_FETCH`), but the model list has a second source: a **build-time
embedded snapshot**. `packages/opencode/script/build.ts` bakes the constant
`OPENCODE_MODELS_DEV` from `script/generate.ts`, which fetches the **entire** `models.dev`
catalog *at build time* and compiles it into the binary
(`packages/core/src/models-dev.ts` → `loadSnapshot`). So the cloud catalog ships *inside* the
binary; disabling the runtime fetch never removed it.

**License.** OpenCode is MIT. Rebranding and stripping the catalog are permitted, provided the
MIT license + copyright notice are retained (OC-016).

## 2. Principle reconciliation — "bundle, don't fork" still holds

CLAUDE.md §1 says *do not fork or rebuild OpenCode*. This suite does **not** fork it or
reimplement the harness (tool-calling, file editing, the TUI). It applies a **minimal,
reviewable set of build-time hardening + rebranding patches over the pinned upstream
(`OPENCODE_REF`)**, reapplied on each upstream bump (OC-017) — the same "own the hardening +
distribution, delegate the harness" split aegis already lives by (OC-002/003/006). The
distinction codified here: *configure + patch the build we already produce* ≠ *maintain a
divergent fork*.

## 3. Requirements

### REQ-OC-012 — Whitelisted model catalog (no cloud/commercial models)
**The built OpenCode shall** embed a model catalog containing ONLY aegis-approved local /
whitelisted models — NO cloud/commercial providers (anthropic, openai, google, xai, …). The
model picker lists only the whitelist. *Target:* the binary's embedded catalog has zero
cloud-provider base URLs (`api.anthropic.com`, `api.openai.com`, …) and zero cloud provider
entries; the picker shows only whitelisted models. *Approach:* build with
`OPENCODE_MODELS_URL=file://<aegis-catalog>` (or patch `script/generate.ts`) so
`OPENCODE_MODELS_DEV` is the whitelist, not models.dev. *Test:* `test::TestModelCatalogNoCloud`
(scan the built binary / staged catalog for cloud provider URLs → none). *Depends on:*
`REQ-OC-011`, `REQ-OC-017`.

### REQ-OC-013 — Whitelist is origin-policy-driven
**The model whitelist shall** be derived from aegis's model policy
(`deploy/models/catalog.json` + `deploy/models/origin-policy.json`, MODEL-006/007) so only
origin-allowed, approved models are baked — a PRC-origin or otherwise denied model never enters
the catalog. *Target:* the baked catalog == the approved set; an origin-denied model is absent
from the binary. *Test:* `test::TestWhitelistFromPolicy`. *Depends on:* `REQ-OC-012`,
`REQ-MODEL-007`.

### REQ-OC-014 — Rebrand the app to "aegis"
**The user-visible app shall** present as "aegis": the TUI title/header
(`packages/tui/src/app.tsx`), the CLI `scriptName` + bin (`packages/opencode/src/index.ts`,
`package.json`), the logo/banner (`packages/opencode/src/cli/ui.ts`), and the HTTP user-agent
(`build.ts` `--user-agent`). *Target:* the launched TUI + `aegis --help` show aegis branding,
not "OpenCode". *Test:* `test::TestHarnessRebranded` (built binary's branding strings are
aegis; no user-visible "OpenCode"). *Depends on:* `REQ-OC-017`.

### REQ-OC-015 — Aegis docs/help, offline (not opencode.ai)
**The in-binary docs/help shall** reference aegis, not `opencode.ai/docs`. The `docs` command +
help banner point at aegis documentation, and — air-gap — must not depend on an external URL as
the only path (prefer embedded/offline docs or a local path). *Target:* no `opencode.ai/docs`
surfaced to the operator; help reflects aegis. *Test:* `test::TestHarnessDocsAegis`. *Depends
on:* `REQ-OC-014`.

### REQ-OC-016 — MIT attribution preserved
**The rebranded distribution shall** retain OpenCode's MIT license + copyright notice (a
`THIRD-PARTY-NOTICES` / `NOTICE` crediting anomalyco/opencode + the MIT text), so the rebrand is
lawful. *Target:* the bundle + repo carry OpenCode's MIT license + copyright. *Test:*
`test::TestOpenCodeAttribution`. *Depends on:* `REQ-OC-014`.

### REQ-OC-017 — Build-time patch set over pinned upstream (not a fork)
**The rebrand + whitelist shall** be applied as a minimal, reviewable patch set / build-time
transform in `deploy/opencode/patches/` (or an injection in `build-opencode.sh`) that applies
cleanly to `OPENCODE_REF` and is reapplied on upstream bumps (OC-008) — NOT a maintained fork.
*Target:* `scripts/build-opencode.sh` applies the patches to the pinned checkout before build;
a bump re-applies (or flags a conflict) rather than diverging. *Test:* `test::TestHarnessPatchSet`
(patches apply to the pinned source; build-opencode.sh references them). *Depends on:*
`REQ-OC-005`.

### REQ-OC-018 — ITAR / air-gap verification of the harness
**The harness shall** prove, under the egress gate, that the rebranded OpenCode (a) lists ONLY
whitelisted models, (b) makes NO network egress beyond loopback, and (c) exposes no cloud
provider base URL as a selectable option. *Target:* `verify-airgap.sh -- aegis` (TUI bootstrap)
shows only whitelisted models + EGRESS=0; the binary contains no selectable cloud API base URL.
*Test:* `test::TestHarnessAirgapITAR` (extends GUARD-001 with the catalog/cloud-URL check).
*Depends on:* `REQ-OC-012`, `REQ-OC-014`, `REQ-GUARD-001`.

## 4. Notes

- This is **out-of-enclave build hardening** — the patches + whitelist are applied on the
  connected build host (OC-003 reproducible build), and the resulting binary is what ships into
  the enclave. The whitelist is the SAME governance as the model bundle (MODEL-006/007).
- `OC-012` is the headline (the observed gap). `OC-017` is foundational (the patch mechanism
  the others ride on). Build `OC-017` → `OC-012` → `OC-014` first; `OC-018` is the closing gate.
- CLAUDE.md §1/§2 will be updated to state the patch-not-fork nuance (OC-017 codifies it).
