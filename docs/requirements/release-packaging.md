# Requirement Specification — Reproducible Signed Release + SBOM

**Thread:** `BUILD-002..007` · **Phase / sprint v0.5** · Status: PLANNED
**Tracked in:** `.rtmx/database.csv` · **Skills:** `airgap-hygiene`, `go-conventions`, `build-to-spec`, `rtmx-loop`

## 1. Purpose & scope

aegis-cli builds today (`make build`, `BUILD-001` proves the offline/vendored
build succeeds and ships std-lib-only) but has **no release or packaging path**:
no cross-compiled artifacts, no SBOM, no checksums, no signatures, no
tag-triggered workflow. This thread specifies a **reproducible, offline, signed
release with a CycloneDX SBOM** — an artifact set built once on a connected
build host, then transferred into a closed enclave on a stage-then-disconnect
basis and **verified in-enclave with no online dependency**.

In scope: a cross-compiled static release build for the ship targets
(`linux-cpu` → linux/amd64 + linux/arm64; `darwin-metal` → darwin/arm64 Apple
Silicon + darwin/amd64 Intel; plus windows/amd64), Debian (.deb) packages for the
Linux targets, a CycloneDX SBOM derived from the vendored module set, a
SHA-256 checksums manifest, offline detached signatures over that manifest, the
reproducibility procedure for a given commit, and the tag-triggered workflow
that emits the full set. Out of scope: OS-native packaging (`.msi`/`.deb`/`.rpm`
from the legacy Rust repo — aegis-cli ships a single static binary, not OS
packages), keyless/online signing (cosign keyless needs an online Fulcio CA +
Rekor transparency log — forbidden by the air-gap posture; see §4), and any
in-enclave network fetch during install (the bundle is self-contained).

The shipped binary is std-lib-only (enforced by `TestRuntimeBinaryIsStdLibOnly`),
so the "supply chain" the SBOM and signatures protect is the **build inputs and
the artifacts in transit**, not a runtime dependency graph. The SBOM therefore
records the vendored module set — including test-only deps such as `godog` whose
provenance must be auditable even though they never link into `cmd/aegis`.

**Validation is by inspection.** Like the `GUARD`/`CI` requirements in this repo,
the heavy cross-build and signing run only in the tag-triggered release workflow,
not per-PR. The acceptance tests live in `test/` (package `offline`) and assert
that the scripts, Makefile targets, and workflow **exist and cover the required
steps** — they do not perform a full cross-build on every PR.

## 2. Definitions

- **Release artifact** — a per-target static binary produced by the release
  build, named for its target (e.g. `aegis-<version>-linux-amd64`,
  `-linux-arm64`, `-darwin-arm64`).
- **Artifact set** — the complete release output: all release binaries, the
  CycloneDX SBOM, the `SHA256SUMS` manifest, and the detached signature(s) over
  that manifest.
- **Ship targets** — the two procured hosts from `docs/hardware-purchase-spec.md`:
  `linux-cpu` (→ linux/amd64 + linux/arm64), `darwin-metal` (→ darwin/arm64 Apple
  Silicon + darwin/amd64 Intel), and windows/amd64.
- **Checksums manifest** — a `SHA256SUMS` file with one `sha256  filename` line
  per non-signature artifact (every binary + the SBOM), in the standard
  `sha256sum -c` format.
- **Offline-verifiable signature** — a detached signature (minisign or GPG) over
  the checksums manifest that an in-enclave verifier can check with only the
  artifact, the signature, and a pre-staged public key — no CA, OCSP, or
  transparency-log lookup.
- **Reproducible build** — building the same commit under the pinned toolchain
  and the same flags yields **byte-identical** binaries.
- **Inspection test** — a `test/` (package `offline`) test that asserts the
  release machinery (script/target/workflow) is present and covers the required
  steps, without executing a full cross-build per run.

## 3. Requirements

Each requirement is well-formed (EARS style), independently verifiable, and
linked to the inspection test that closes it via `rtmx verify`.

### REQ-BUILD-002 — Reproducible static cross-compiled release build
**The release build shall** cross-compile a static binary for each ship target
(linux/amd64, linux/arm64, darwin/amd64, darwin/arm64, windows/amd64) with
`CGO_ENABLED=0`, `-trimpath`, and
the version + commit stamped via `-ldflags -X`, producing one named artifact per
target.
*Rationale:* the two procured hosts (`docs/hardware-purchase-spec.md`) need a
single static binary each; CGO-off + `-trimpath` are prerequisites for the
std-lib-only and reproducibility invariants (§4). Extends the offline build
already proven by `BUILD-001`.
*Acceptance:* `make release` (delegating to `scripts/release.sh`) is configured
to build all three targets with `CGO_ENABLED=0`, `-trimpath`, and version+commit
`-ldflags`; an inspection of the release machinery confirms the three targets and
the flags; the tag-triggered cross-build is green.
*Test:* `test::TestReleaseBuildMatrixConfigured`. *Depends on:* REQ-BUILD-001.

### REQ-BUILD-003 — CycloneDX SBOM from the vendored module set
**The release build shall** generate, per release, a valid CycloneDX SBOM that
lists the vendored module set, including the provenance of test-only
dependencies (e.g. `godog`).
*Rationale:* the shipped binary is std-lib-only, but the build inputs are not;
auditors entering the enclave need a machine-readable bill of materials of every
module that was vendored, test-only deps included. CycloneDX is a native data
format (per repo convention — data in JSON, not Markdown).
*Acceptance:* `make sbom` (via `scripts/release.sh`) emits a CycloneDX document
(e.g. `aegis-<version>.cdx.json`) that parses as valid CycloneDX and enumerates
the vendored deps, including `godog`'s provenance; an inspection of the machinery
confirms SBOM generation from the vendored set.
*Test:* `test::TestSBOMGenerationConfigured`. *Depends on:* REQ-BUILD-002.

### REQ-BUILD-004 — SHA-256 checksums manifest over every artifact
**The release build shall** produce a single `SHA256SUMS` manifest with a
SHA-256 entry for each release binary and for the SBOM.
*Rationale:* a single, signable digest list is the integrity anchor for transfer
into the enclave; signing one manifest (REQ-BUILD-005) covers the whole set
transitively.
*Acceptance:* `make release` writes a `SHA256SUMS` file in standard
`sha256sum -c` format with one line per binary + the SBOM; an inspection
confirms the manifest covers every non-signature artifact and is `-c`-checkable.
*Test:* `test::TestChecksumsManifestConfigured`. *Depends on:* REQ-BUILD-002.

### REQ-BUILD-005 — Offline detached signatures, verifiable in-enclave
**The release build shall** produce detached signature(s) (minisign or GPG) over
the `SHA256SUMS` manifest that an in-enclave verifier can check offline, with no
online CA or transparency-log dependency.
*Rationale:* the air-gap posture forbids any verify-time network call; keyless
signing (cosign + Fulcio/Rekor) requires an online CA and transparency log and
is therefore excluded (§4). A detached minisign/GPG signature over the manifest
is verifiable with only a pre-staged public key.
*Acceptance:* `make sign` (via `scripts/release.sh`) emits a detached signature
(e.g. `SHA256SUMS.minisig` and/or `SHA256SUMS.asc`) over the manifest; an
inspection confirms the signature is detached, covers the checksums manifest, and
the documented verify path uses only a local public key (no CA/OCSP/Rekor).
*Test:* `test::TestReleaseSigningConfigured`. *Depends on:* REQ-BUILD-004.

### REQ-BUILD-006 — Offline, vendored, reproducible for a given commit
**The release build shall** build offline from the vendored tree
(`GOPROXY=off`, `-mod=vendor`) and **shall** yield byte-identical binaries when
the same commit is rebuilt under the pinned toolchain.
*Rationale:* reproducibility lets an enclave operator (or auditor) rebuild the
exact artifact from source and confirm it matches what was transferred — the
strongest provenance claim available without trusting the build host. Inherits
the offline invariant `BUILD-001` already enforces.
*Acceptance:* the release build sets `GOPROXY=off` and `-mod=vendor`; the
reproducibility procedure is documented and scripted (`scripts/release.sh`
supports a verify/repro mode), addressing `SOURCE_DATE_EPOCH` and `-buildvcs`
considerations; an inspection confirms the offline flags and the documented
identical-rebuild procedure.
*Test:* `test::TestReleaseIsOfflineReproducible`. *Depends on:* REQ-BUILD-002.

### REQ-BUILD-008 — Debian packages for the Linux targets
**The release build shall** produce Debian (`.deb`) packages for the Linux ship
targets (amd64, arm64) via `dpkg-deb`, installing the binary to `/usr/bin/aegis`.
*Rationale:* `.deb` is the native install path for Debian/Ubuntu enclave hosts;
it rides the same offline, checksummed, signed artifact set. *Acceptance:*
`aegis_<version>_amd64.deb` and `_arm64.deb` are built and covered by the
checksums manifest. *Test:* `test::TestDebianPackagingConfigured`. *Depends on:*
REQ-BUILD-002.

### REQ-BUILD-007 — Tag-triggered release workflow emits the full set
**When** a version tag is pushed, **the release workflow shall** build the
per-target binaries, generate the SBOM, write the checksums manifest, and produce
the detached signature(s), publishing the complete signed artifact set.
*Rationale:* one tag → one reproducible, signed, SBOM'd artifact set is the
operator-facing release ritual; mirrors the `ci.yml` toolchain-install + `make`
delegation style so the workflow stays a thin driver over the Makefile/script.
*Acceptance:* `.github/workflows/release.yml` triggers on a tag, installs the
toolchain, and runs the release targets (`make release`/`sbom`/`sign`, via
`scripts/release.sh`) to emit binaries + SBOM + `SHA256SUMS` + signature(s); an
inspection confirms the tag trigger and that all four artifact classes are
produced.
*Test:* `test::TestReleaseWorkflowConfigured`.
*Depends on:* REQ-BUILD-003, REQ-BUILD-004, REQ-BUILD-005.

## 4. Design constraints

- **Single static binary, CGO off.** The release build sets `CGO_ENABLED=0` and
  `-trimpath` so the shipped artifact stays std-lib-only and portable, preserving
  the invariant enforced by `test.TestRuntimeBinaryIsStdLibOnly`.
- **Offline / vendored.** The release build runs under `GOPROXY=off` and
  `-mod=vendor`, reusing the offline-build discipline already proven by
  `BUILD-001` / `test.TestOfflineBuildSucceeds`. No release step may fetch from
  the network at build time, and **no artifact may require a network fetch at
  install/verify time in-enclave**.
- **Air-gap-first signing — offline detached signatures, not keyless.** Signing
  uses detached minisign/GPG over the checksums manifest, verifiable with a
  pre-staged public key. Keyless/online signing (cosign keyless → Fulcio CA +
  Rekor transparency log) is **explicitly rejected**: it needs an online CA and a
  public transparency log at sign and verify time, which the closed enclave
  cannot reach. This trade-off — losing keyless's no-key-management convenience
  to keep offline verifiability — is deliberate and load-bearing.
- **SBOM from the vendored set.** The CycloneDX SBOM is derived from `vendor/`
  (the actual build inputs), not from a live module query, so it is itself
  produced offline and records test-only deps (`godog`) for provenance.
- **Reproducibility levers.** Reproducibility is achieved with `-trimpath`, a
  pinned toolchain (matching `ci.yml`'s Go version), `GOPROXY=off -mod=vendor`,
  and explicit handling of `SOURCE_DATE_EPOCH` and `-buildvcs` (VCS stamping is
  controlled so it does not non-deterministically perturb the binary); version +
  commit are injected via `-ldflags -X` rather than read from a dirty tree.
- **Planned implementation artifacts.** `scripts/release.sh` (cross-build, SBOM,
  checksums, sign, repro-verify); Makefile `release` / `sbom` / `sign` targets
  delegating to it (mirroring how `build` stamps `LDFLAGS` and selects
  `GO_BUILD_ENV` vendor/offline mode today); and `.github/workflows/release.yml`
  as a thin tag-triggered driver that installs the toolchain and calls those
  targets — same Makefile-as-single-source-of-truth pattern as `ci.yml`.
- **Inspection tests in `test/` (package `offline`).** The closing tests
  (`TestReleaseBuildMatrixConfigured`, `TestSBOMGenerationConfigured`,
  `TestChecksumsManifestConfigured`, `TestReleaseSigningConfigured`,
  `TestReleaseIsOfflineReproducible`, `TestReleaseWorkflowConfigured`) live
  alongside the existing offline/CI inspection tests and assert presence +
  coverage of the release machinery, keeping per-PR runs fast.

## 5. Verification & exit criteria

The thread is complete when all six requirements are `COMPLETE` via
`rtmx verify --update`, `rtmx health` is HEALTHY at 100% coverage, and `make ci`
is green (race, lint, govulncheck, cover-gate ≥ floor, EGRESS=0, TRACE=100%,
ACR-regression). Build order follows the dependency graph:
REQ-BUILD-002 first; then REQ-BUILD-003 / REQ-BUILD-004 / REQ-BUILD-006 in
parallel; then REQ-BUILD-005; then REQ-BUILD-007 once the SBOM, checksums, and
signing requirements it depends on are closed.
