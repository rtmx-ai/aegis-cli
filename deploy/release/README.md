# Release signing keys

The **public** key used to verify aegis-cli releases lives here. `make
verify-release` and the procedure in `docs/release-signing.md` use it.

- `aegis-minisign.pub` — minisign public key (preferred), OR
- `aegis-gpg.pub` — GPG public key.

Provision a keypair per `docs/release-signing.md` §1 and commit ONLY the public
key here. The secret key stays on a controlled host (and, for enclave releases,
never touches CI). Key custody is the security/export-control authority's call.

No project key is committed yet — releases are unsigned until one is provisioned.
