# Windows support — build + install targets

aegis targets linux-cpu and darwin-metal today. This adds **Windows** as a first-class platform:
`windows/amd64` (= x86-64 — the same arch, one target) and `windows/arm64`. (32-bit x86 is out of scope
unless a customer requires it.) All four install mechanisms are in scope (owner decision): signed portable
`.zip`, winget, Scoop, and MSI.

## Reality check — what's easy vs. hard

- **The aegis binary** cross-compiles to Windows trivially (pure Go, CGO-free) — WIN-001, buildable now.
- **The bundle is the work.** aegis ships `llama-server` (native C++ — needs a Windows build, CPU or a
  GPU backend such as Vulkan), OpenCode (bun — cross-platform-ish), and ripgrep (Windows binary exists).
  A Windows `llama-server` build is the load-bearing effort, and (like darwin) it must run on a Windows
  CI runner.
- **Air-gap hardening is Linux-specific.** The EGRESS=0 netns gate and the bubblewrap sandbox don't exist
  on Windows; the ITAR controls need a Windows equivalent (AppContainer / Job Objects / WFP firewall).
  Non-negotiable #1 (closed by construction) must hold on Windows too — a Windows release cannot ship
  until its egress gate exists.

## Requirements

### WIN-001 — aegis binary cross-compiles to Windows *(buildable now)*
`GOOS=windows GOARCH={amd64,arm64} go build` produces a working `aegis.exe`, CGO-free, from the vendored
tree. *Test:* `internal/... / test::TestWindowsCrossCompile`

### WIN-002 — Windows platform bundle
Build `llama-server` for Windows (CPU baseline; GPU backend later), stage OpenCode + ripgrep(win) +
`aegis.exe` into a Windows bundle layout. Runs on a `windows-latest` CI runner. Depends on WIN-001.

### WIN-003 — Release builds the Windows platforms
Extend `bundle-matrix.yml` + the release to build `windows/amd64` + `windows/arm64` bundles and attach
them (`.zip`) to the GitHub release, signed + checksummed like the others. Depends on WIN-002. Gated by
the REL-014 platform-completeness rule (a Windows build failure fails the release, or Windows is added to
the required set only once stable).

### WIN-004 — Signed portable `.zip` install *(air-gap-native)*
A minisign-signed `.zip` the operator side-loads + extracts (mirrors the model GGUF staging). The primary
air-gap/ITAR path: no package-manager network dependency. Depends on WIN-003.

### WIN-005 — winget manifest
A winget manifest (`aegis.aegis`) for `winget install`, publishable to the community repo or a private
source. Depends on WIN-003.

### WIN-006 — Scoop bucket
A Scoop manifest published to a bucket (mirrors the Homebrew-tap flow) for `scoop install aegis`.
Depends on WIN-003.

### WIN-007 — MSI installer
A WiX-built, code-signed MSI for enterprise/IT deployment (GPO/Intune). Depends on WIN-003.

### WIN-008 — Windows air-gap controls
The EGRESS=0 guarantee on Windows: a network-deny sandbox for agent-generated code (AppContainer / Job
Object + WFP) and an egress gate for CI, equivalent to the netns/bubblewrap controls. Blocks any Windows
release (non-negotiable #1). Depends on WIN-001.

### SITE-012 — OS-aware download links
The rtmx.ai aegis page resolves the download to the **latest release asset for the visitor's OS/arch**
(macOS/Linux/Windows) — OS detection + the GitHub Releases API (or a `/releases/latest`-anchored link),
replacing today's `<version>` placeholders. Depends on WIN-003 (so Windows assets exist to link).

## Sequencing
WIN-001 (binary) → WIN-002 (bundle, needs Windows CI) → WIN-003 (release matrix) → WIN-004..007 (the four
install paths) + WIN-008 (air-gap, gates the release) → SITE-012 (download UX). WIN-001 ships now; the rest
need a Windows build loop and the `llama-server` Windows build to be proven (the risk item, like darwin).
