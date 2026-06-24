# Requirement Specification — OpenCode Integration (Build Our Own, Hardened)

**Thread:** `OC-001..005` · **Phase 7 / sprint v0.3** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `airgap-hygiene`, `go-conventions`, `build-to-spec`

## 1. Purpose & scope

aegis is built **around OpenCode** (`anomalyco/opencode`, MIT) as its centerpiece
TUI. Rather than consume upstream prebuilt binaries (whose supply chain we don't
control), aegis **builds OpenCode itself, from pinned source, with air-gap
protections compiled into the build**, and ships it inside the aegis release —
signed, SBOM'd, checksummed, offline. This is the foundation thread for that
integration; the launch/UX is the TUI thread (`TUI-001..006`).

`anomalyco/opencode` is a Bun/TypeScript monorepo (`packages/cli` is the agent)
that compiles to a **single self-contained binary** (`bun build --compile`), so
the built artifact needs no Bun/Node runtime in the enclave. In scope: the source
pin, the hardened build, the offline/reproducible/zero-egress build proof,
resolving + launching the self-built binary, and bundling it (with its SBOM) into
the signed release. Out of scope: forking/modifying OpenCode's behavior (we pin,
build, and harden it — we do not change its features) and the TUI launch UX
(covered by the TUI thread).

## 2. Definitions

- **Pinned source** — a specific `anomalyco/opencode` tag/commit recorded in
  `deploy/opencode/OPENCODE_REF`; the only ref the build will compile.
- **Hardened build** — a build that disables OpenCode's egress vectors
  (telemetry, autoupdate, share, analytics, model-registry fetch) at build time
  *and* in the shipped config — defense in depth.
- **Self-built binary** — `deploy/opencode/bin/opencode`, produced by
  `scripts/build-opencode.sh` on the connected build host (stage-then-disconnect).

## 3. Requirements

### REQ-OC-001 — Pinned source
**aegis shall** pin `anomalyco/opencode` at a specific source ref recorded in
`deploy/opencode/OPENCODE_REF`, so the OpenCode it builds is reproducible and
auditable. *Acceptance:* the pin file exists, names a concrete tag/commit, and
the source is `anomalyco/opencode`. *Test:* `test::TestOpenCodePinned`.
*Depends on:* REQ-TUI-001.

### REQ-OC-002 — Hardened build from source
**aegis shall** build OpenCode from the pinned source into a single
self-contained binary, with air-gap protections baked in: telemetry, autoupdate,
share, and analytics disabled in the build and the shipped config. *Acceptance:*
`scripts/build-opencode.sh` checks out the pin, compiles `packages/cli` to a
standalone binary, and applies the hardening. *Test:*
`test::TestBuildOpenCodeConfigured`. *Depends on:* REQ-OC-001.

### REQ-OC-003 — Offline, reproducible, zero-egress build
**The OpenCode build shall** install dependencies offline from a frozen lockfile
(no live fetch during the build), be reproducible for the pinned ref, and the
built binary shall make zero non-loopback egress. *Acceptance:* the build uses
`bun install --frozen-lockfile` against a staged dependency cache, and the binary
passes the EGRESS=0 gate (`scripts/verify-airgap.sh`). *Test:*
`test::TestOpenCodeBuildIsOfflineHardened`. *Depends on:* REQ-OC-002, REQ-GUARD-001.

### REQ-OC-004 — Resolve + launch the self-built binary
**aegis shall** resolve and launch the self-built OpenCode
(`deploy/opencode/bin/opencode`) — in addition to PATH and alongside the aegis
binary — under the hardened config (loopback model + rtmx MCP + offline).
*Test:* `internal/opencode::TestResolveStaged`. *Depends on:* REQ-OC-002.

### REQ-OC-005 — Bundle into the signed release
**The release shall** build OpenCode from source, bundle the self-built binary
alongside aegis, include its SBOM, and cover it with the checksums manifest and
signature. *Acceptance:* `release.sh` invokes the OpenCode build, stages the
binary into the artifact set, and the SBOM/checksums cover it. *Test:*
`test::TestReleaseBuildsOpenCode`. *Depends on:* REQ-OC-002, REQ-BUILD-003.

## 4. Design constraints

- Build, don't fork (CLAUDE.md §1): we compile pinned OpenCode source and harden
  it; we do not change its features. The build lives in `scripts/build-opencode.sh`.
- The self-built binary is a single self-contained executable (Bun `--compile`),
  so the enclave needs no Bun/Node runtime — keeping the air-gap distribution clean.
- The real build requires the Bun toolchain on the connected build host; like the
  real-model and live-TUI steps, the actual compile is a **gated** host step. The
  build script, pin, resolution, release wiring, and config are unit/inspection-
  tested here; the compile is validated on a Bun-equipped host.
- Supply chain: pinned source + offline frozen deps + SBOM + signature + EGRESS=0
  — the same posture aegis applies to itself, extended to the bundled OpenCode.

## 5. Verification & exit criteria

All five COMPLETE via `rtmx verify`, `rtmx health` HEALTHY, `make ci` green.
Build order: OC-001 → OC-002 → OC-003 ∥ OC-004 → OC-005. A real hardened build on
a Bun host (producing `deploy/opencode/bin/opencode`, EGRESS=0) is the gated
validation, then bundled into the next signed release.

## Addendum — OpenCode 2.0 preview finding (post-build validation)

Building `anomalyco/opencode @ v1.17.9` for real revealed it is **OpenCode 2.0
preview** (CLI: `serve`/`service`/`debug`/`migrate`; binary `lildax`, 128 MB,
self-contained, EGRESS=0 verified). Two follow-ups (PLANNED):

- **REQ-OC-006 — v2 model/rtmx wiring.** OpenCode 2.0 reads `opencode.json` and
  `opencode serve` starts cleanly with our config (config accepted), but the v2
  provider schema (`providers` record, Effect-Schema, AISDK/Native model API
  variants) is undocumented and differs from the v1 `provider.options.baseURL`
  style our config uses. The config must be aligned to v2 and validated by a real
  completion routed to the loopback model via `opencode serve` (non-interactive,
  testable) — gated, like real-model validation.
- **REQ-OC-007 — CI build + bundle.** `release.yml` must install Bun and run
  `build-opencode.sh` so the signed release bundles the self-built OpenCode per
  ship platform (today's `--single` build covers the build host's platform only).

Decision pending: stay on the 2.0 preview (do the v2 wiring) vs. pin a config-
stable OpenCode line. The self-built/hardened/EGRESS-0 foundation holds either way.
