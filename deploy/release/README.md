# Release signing keys

The **public** key used to verify aegis-cli releases lives here. `make
verify-release` and the procedure in `docs/release-signing.md` use it.

- `aegis-minisign.pub` — minisign public key (preferred), OR
- `aegis-gpg.pub` — GPG public key.

Provision a keypair per `docs/release-signing.md` §1 and commit ONLY the public
key here. The secret key stays on a controlled host (and, for enclave releases,
never touches CI). Key custody is the security/export-control authority's call.

Provisioned (REL-001): `aegis-minisign.pub` (minisign key `28F95ACEED83B1BA`). The secret
half is held off-repo as a host/CI secret (`MINISIGN_KEY`) and never committed; `scripts/
release.sh` signs `SHA256SUMS` with it, and `make verify-release` verifies against the public
key here.
