---
name: airgap-hygiene
description: Write code that makes ZERO external network calls — only loopback to the local model endpoint is permitted — and be able to prove it. Use this whenever you add a dependency, touch config, write anything that could open a socket, or wire up CI's egress gate. Trigger it on any import, any URL, any default that could phone home, and any time you need to demonstrate the build is closed.
---

# airgap-hygiene

This is non-negotiable #1 (`CLAUDE.md` §1) expressed as engineering discipline: aegis-cli
runs in a closed, ITAR-suitable environment, so nothing it ships or writes may make a
network call other than loopback to the local model endpoint. **Egress is a build-failing
condition, not a warning** (`GUARD-001`). Treat every line you write as if a packet capture
is watching — because in CI, one is.

## The rules

- **Zero egress.** The only allowed destination is loopback to the local serving endpoint
  (`127.0.0.1`/`::1`). DNS, telemetry, update checks, package fetches, "anonymous" pings,
  CDN asset pulls — all forbidden, all build-failing.
- **Vendored / std-lib-only deps.** Prefer the standard library. Any dependency is
  vendored (`vendor/`) so the build needs no live fetch; CI builds with the network
  disabled to prove it (`BUILD-001`). A dep that itself phones home is disqualified.
- **Offline-safe defaults.** If a setting *could* cause egress, its default is off
  (`internal/config`, validated). Harnesses and servers are pre-hardened in `deploy/`:
  opencode `offline=true` + share/telemetry off, Ollama update-check off, default-deny
  egress firewall rules. Never rely on the operator to remember to disable a phone-home.

## Prove it, don't assert it

"It doesn't make calls" is a claim; the gate is the proof. `scripts/verify-airgap.sh` runs
the loop under packet capture and **fails if any non-loopback packet leaves** — this is the
CI egress gate and the `EGRESS=0` hard gate in the metrics run. Run it locally before you
trust a change:

```bash
scripts/verify-airgap.sh -- aegis run --once
```

If you can't explain every destination the capture shows, the change isn't done. A new
dependency, a new client, a new config default — each is a potential egress vector; close
it and re-prove.

## The headroom caution

If context compression is ever revisited (`CLAUDE.md` §9), its egress vectors — PyPI update
check, HF/cdn.pyke asset fetches — must be locked behind this gate *before* adoption. That
deferred decision is exactly why this discipline is reflexive: any new component is guilty
of egress until the capture proves it innocent.

## Example

Adding an HTTP client for the model endpoint: bind it to the configured loopback address,
no proxy env honored, no DNS lookup (numeric host), dependency vendored. Then
`scripts/verify-airgap.sh -- aegis run --once` — capture shows only `127.0.0.1` traffic to
the serving port, zero outbound. Green. Had the client resolved a hostname or honored
`HTTPS_PROXY`, the capture would show a stray packet and CI would fail the build. Closed,
and proven closed.
