# Release Signing & Verification

How aegis-cli releases are signed and how to verify one before trusting it —
especially before importing a release into a closed enclave. Signing is
**offline and detached** by design (minisign or GPG); we deliberately avoid
keyless/online schemes (e.g. transparency-log-backed signing) that need an
online CA at sign *and* verify time, which an air-gapped host cannot reach.

> Key custody is the security/export-control authority's decision (see the
> Technology Control Plan). This document is the procedure and tooling; it does
> not ship a project signing key. v0.2.0 was published **unsigned**; provision a
> key per §1 and subsequent releases sign automatically.

## What is signed

`scripts/release.sh` (`make release`) produces `dist/SHA256SUMS` — a manifest
covering every artifact (binaries, `.deb`, SBOM). The **detached signature is
over `SHA256SUMS`**, so one signature transitively authenticates every artifact
(verify the signature, then verify each file against the manifest).

## 1. Generate a keypair (once, offline)

**minisign** (recommended — small, single-purpose, offline):

```bash
minisign -G -p deploy/release/aegis-minisign.pub -s ~/.aegis/aegis-minisign.key
```

Commit the **public** key (`deploy/release/aegis-minisign.pub`); keep the secret
key off the repo and off CI for enclave releases (on a controlled host only).

**GPG** (alternative): `gpg --full-generate-key`, then export the public key to
`deploy/release/aegis-gpg.pub` and distribute it through a trusted channel.

## 2. Sign a release

`release.sh` signs automatically when a key is available:

- **minisign:** set `MINISIGN_KEY=<path to secret key>` → writes
  `dist/SHA256SUMS.minisig`.
- **GPG:** set `GPG_KEY=<key id>` → writes `dist/SHA256SUMS.asc`.

```bash
MINISIGN_KEY=~/.aegis/aegis-minisign.key make release   # offline, on the release host
```

For the **public** GitHub releases, the same can run in CI by providing the
secret key as the `MINISIGN_KEY`/`GPG_KEY` Actions secret (the public-artifact
trust anchor is the project CI). For **in-enclave controlled** releases, sign
offline on the controlled host — never put that key in CI.

## 3. Verify a release (before trusting it)

```bash
make verify-release        # verifies the detached signature + every checksum
```

This (a) verifies `SHA256SUMS.minisig`/`.asc` against the trusted public key in
`deploy/release/`, then (b) re-hashes every artifact against `SHA256SUMS`. Both
must pass. Manual equivalent:

```bash
minisign -Vm dist/SHA256SUMS -p deploy/release/aegis-minisign.pub   # or: gpg --verify dist/SHA256SUMS.asc dist/SHA256SUMS
( cd dist && sha256sum -c SHA256SUMS )
```

A release with no valid signature, or any checksum mismatch, must be rejected —
treat it as untrusted and do not import it into the enclave.
