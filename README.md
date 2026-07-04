# aegis-cli

[![CI](https://github.com/rtmx-ai/aegis-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/rtmx-ai/aegis-cli/actions/workflows/ci.yml)
[![coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/rtmx-ai/aegis-cli/badges/coverage.json)](https://github.com/rtmx-ai/aegis-cli/actions/workflows/ci.yml)
[![version](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/rtmx-ai/aegis-cli/badges/version.json)](https://github.com/rtmx-ai/aegis-cli/blob/main/VERSION)
[![Go Report Card](https://goreportcard.com/badge/github.com/rtmx-ai/aegis-cli)](https://goreportcard.com/report/github.com/rtmx-ai/aegis-cli)
[![License](https://img.shields.io/github/license/rtmx-ai/aegis-cli)](LICENSE)

**aegis is an air-gap-native, top-tier agentic coding experience for closed
environments.** Running `aegis` launches a hardened **OpenCode TUI** (MIT), driven by a
**local model** (llama.cpp / Ollama, loopback only), with **rtmx as the intent layer**.
It makes zero network calls beyond loopback to the local model endpoint — *by
construction, not by configuration*.

aegis **bundles and launches OpenCode; it does not fork or rebuild it.** Tool-calling,
file editing, and sandboxing are OpenCode's job. aegis owns what a closed-enclave
distribution needs *around* the harness: air-gap hardening + egress default-deny, the
rtmx intent loop (interactive in the TUI and headless via `aegis run`), audit, metrics,
host calibration, and packaging.

```mermaid
flowchart LR
    user["operator"] --> tui["aegis → OpenCode TUI (bundled, hardened)"]
    tui <--> model["local model (llama.cpp / Ollama, loopback)"]
    tui <--> rtmx["rtmx MCP — intent layer (next/claim/verify/set_status)"]
    tui -.headless.-> loop["aegis loop — unattended rtmx drain"]
```

---

## Install

aegis ships as a single static binary plus a bundled harness (OpenCode + ripgrep +
llama-server). The large model GGUF is **side-loaded separately** — it is too big to
package, and an air gap wants it staged deliberately anyway.

```bash
# Homebrew — macOS (Apple Silicon) and Linux
brew install rtmx-ai/tap/aegis

# Debian / Ubuntu — grab aegis_<version>_<arch>.deb from the latest release, then:
sudo apt install ./aegis_<version>_amd64.deb  # installs aegis + the harness under /usr/lib/aegis

# Build everything from source on a connected build host (full stack):
git clone https://github.com/rtmx-ai/aegis-cli && cd aegis-cli
./setup.sh                                    # aegis + OpenCode + llama.cpp, stages a model, calibrates

# ...or build just the aegis binary (offline, vendored — proves no live fetch):
make build                                    # → ./bin/aegis
```

Prebuilt downloads (binaries, `.deb`s, the Homebrew tarballs, the CycloneDX SBOM, and a
minisign-signed `SHA256SUMS`) are attached to each [GitHub
release](https://github.com/rtmx-ai/aegis-cli/releases); verify them with `make
verify-release` against the public key in `deploy/release/`. After installing, stage a
model (see [Setup](#setup)) and run `aegis verify-env` to confirm the environment is
closed and traceable.

---

## What aegis is — and where it came from

aegis began as a Rust pair-programmer aimed at compliance-bound, cloud-managed
environments. Trying to *be* the harness — and tethering the whole thing to a managed
cloud — did not pan out. aegis was rebuilt ground-up in Go around a sharper conviction:
the hard part of agentic coding in a secure environment isn't the cloud, it's the **air
gap**. Everything that makes a cloud assistant convenient — model APIs, telemetry,
plugin fetches, auto-update — is exactly what a closed enclave forbids.

So aegis became **air-gap-native**: a single static Go orchestrator that runs entirely on
one closed host, bundles a best-in-class open harness instead of reinventing it, drives a
local model over loopback, and tracks *intent* with rtmx so a small on-host model can
close real work one verifiable requirement at a time. Egress isn't a setting you turn off
— it's a build-failing condition. That is the whole point, and the rewrite is built so it
cannot drift.

---

## Setup

One command builds the full stack from pinned source (aegis + OpenCode + llama.cpp),
stages + verifies the model, calibrates serving to the host, and smoke-tests the whole
stack — run it on a connected build host:

```bash
./setup.sh                                  # menu of catalog models (auto-selects the recommended one)
./setup.sh --model-choice gemma-4-26b-a4b   # download a specific catalog model
./setup.sh --model /path/to/model.gguf      # or use a local GGUF
./setup.sh --install                        # also install `aegis` to ~/.local/bin (on PATH)
```

The catalog menu strikes through any model that won't fit the host's RAM, and
auto-selects the largest one that will. `--install` ends with clear instructions on how
to run `aegis`.

Then install + run in the closed enclave per
[docs/operator-guide.md](docs/operator-guide.md): `aegis` (the OpenCode TUI), `aegis run
"<prompt>"` (one headless task), or `aegis loop` (drain the rtmx backlog). Prerequisites
+ the tiered build cadence are in
[docs/requirements/build-cadence.md](docs/requirements/build-cadence.md).

---

## The three non-negotiables

1. **Closed by construction.** No component aegis-cli ships or writes may make a
   network call other than loopback to the local model endpoint. Egress is a
   *build-failing* condition, not a warning.
2. **Bundle, don't rebuild.** aegis-cli owns distribution, hardening, launch, the rtmx
   intent loop, audit, metrics, the egress guard, and config — and bundles OpenCode + the
   model + rtmx. It does not fork OpenCode or reimplement the harness.
3. **One requirement at a time.** The loop claims a single rtmx requirement, closes it,
   releases it, and moves on. Scope is narrowed so a small local model can succeed and
   every change is independently verifiable.

---

## The stack

| Layer | Choice | Notes |
|---|---|---|
| Model | Host-chosen from a curated catalog | Gemma-4-26B-A4B (MoE, ~4B active — fast on modest hardware) is the default; Devstral-Small (non-PRC agentic coder, Apache-2.0) and others live in `deploy/models/catalog.json`. The host decides — see [Choosing a model](#choosing-a-model-for-the-host). |
| Serving (spike) | Ollama | Fast iteration; localhost-bound; side-loaded GGUF. |
| Serving (prod) | llama.cpp `llama-server` | From source, no telemetry. CPU on Ryzen; Metal on Mac. |
| Harness | opencode (default) / Goose | Swappable behind `internal/harness`. Decide by bake-off. |
| Requirements | rtmx | Static Go binary, CSV-in-git, stdio MCP server. The closed-loop engine. |
| Orchestrator | **aegis-cli (this repo, Go)** | Single static air-gappable binary. |

**Build targets.** Validated first on `linux-cpu` (Ryzen 5950X / Ubuntu / 64 GB) and runs
on `darwin-metal` (MacBook Pro M5 Pro / 24 GB unified). One `calibration.json` (with a
`target` field) plus `internal/serving` drives both, and aegis **sizes the served context
to the host's memory at launch** (24 GB → ~16k, more RAM → 32k) so a big model on a small
box fits instead of overflowing.

### Choosing a model for the host

Model choice is **measured, not assumed** — the host decides, because the binding
constraint is memory (capacity + bandwidth), not preference:

- **`aegis profile`** predicts, for every catalog model that clears the origin policy,
  whether it fits this host's memory and how fast it would decode (tok/s), and recommends
  the largest that clears an interactive/unattended throughput floor.
- **`aegis bakeoff`** then *measures* the host-suitable models head-to-head on a fixed
  suite of real coding tasks — scoring **files-edited** (did it actually write code, the
  agency signal), **ACR** (did the edit pass the test), and real decode **tok/s**. Run it
  bare to pick interactively, or `--all` to download + serve + measure every suitable model.

The trade this surfaces: a dense 24B coder (Devstral) is stronger in the abstract but
reads ~6× the bytes/token of a ~4B-active MoE (Gemma), so on a memory-bandwidth-bound box
the MoE is often both faster *and* the pragmatic pick — while the dense coder wins on
hardware that can feed it. Origin is a control: the catalog records each model's country
of origin and `deploy/models/origin-policy.json` is default-deny (US + an explicit,
auditable FR opt-in for the non-PRC Mistral coder). See [docs/models.md](docs/models.md)
and [docs/model-compliance.md](docs/model-compliance.md).

---

## Quickstart

```bash
# build (offline, vendored — proves no live fetch)
make build           # GOFLAGS=-mod=vendor go build ./cmd/aegis

# run the exact pipeline CI runs: build → vet → unit → airgap gate → golden metrics
make ci

# which catalog models fit THIS host? (memory fit + predicted tok/s per model)
aegis profile

# measure the host-suitable models head-to-head (auto-downloads + serves each):
# files-edited (agency) + ACR + real decode tok/s → a valid, self-documenting comparison
aegis bakeoff --all

# one headless task, one-shot (≡ opencode/ollama run)
aegis run "add a health endpoint and a test"

# verify the environment is closed + traceable before a real run
aegis verify-env

# one loop iteration over the rtmx backlog
aegis loop --once

# drain the backlog unattended (bounded): park-on-escalation, breaker, run budget
aegis loop --max 40 --break-after 3
```

---

## How RTMX drives the closed loop

rtmx is the **foundation** of this project's requirements tracking and closed-loop
verification — it is load-bearing, not decoration. The loop is:

1. **Claim.** `rtmx next` hands the loop the next claimable requirement; the claim is
   atomic (no double-claim) so runs are resumable.
2. **Drive.** The harness implements the minimal change and its tests for that one
   requirement.
3. **`rtmx verify`.** rtmx runs `go test -json ./...` and maps each result back to a
   requirement via its `test_module` (Go package) + `test_function` columns.
4. **Status writeback.** A passing mapped test closes the requirement in
   `.rtmx/database.csv`; a failure downgrades it. That writeback *is* the closed loop.

See progress at any time:

```bash
make rtm        # status + traceability matrix
make backlog    # prioritized backlog (PHASE=<n> to filter a phase)
rtmx health     # the TRACE=100% gate: no orphaned requirements or tests
```

`rtmx health` passing is a **hard CI gate**: traceability must be 100%.

---

## Air-gap / ITAR posture

aegis-cli is developed **in the open** (public `rtmx-ai/aegis-cli`) because it is
orchestrator *tooling*, not the mission work it drives. **No ITAR / CUI or otherwise
controlled data, code, or requirements ever land in this repo.** The controlled work
aegis-cli drives lives only on the internal in-enclave git remote on the closed host —
never in the public org. The `.rtmx/database.csv` here tracks aegis-cli's *own*
(uncontrolled) development requirements — dogfood from commit one.

Egress is treated as a control expressed as a test: any network call beyond loopback
during a run fails the build (`scripts/verify-airgap.sh`). Config defaults are
offline-safe — if a setting could cause egress, its default is off.

---

## Repository layout

See `CLAUDE.md` for the full architecture spec, requirement categories, and CI
metrics. Agent personas and the implementation discipline live in `AGENTS.md` and
`skills/`. Operator and procurement docs are in `docs/`.
