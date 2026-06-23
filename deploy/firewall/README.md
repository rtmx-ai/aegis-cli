# deploy/firewall — default-deny egress (defense-in-depth)

Host firewall rules that **deny all egress except loopback to the local model
port**. Two equivalent rulesets are provided:

- `aegis.nft` — nftables (`sudo nft -f deploy/firewall/aegis.nft`)
- `aegis-iptables.sh` — iptables/ip6tables (`sudo MODEL_PORT=8080 bash deploy/firewall/aegis-iptables.sh`)

## Why this exists — and what it is NOT

The primary air-gap guarantee is **app-level and by construction**: aegis-cli and
every component it ships make no network call other than loopback to the local
model endpoint (`CLAUDE.md` §1), and `scripts/verify-airgap.sh` proves it
(`EGRESS=0` hard gate). This firewall is **defense-in-depth behind** that
guarantee — the belt to the app's suspenders. If some component ever attempted
egress, the kernel drops it here too.

It is **not** a substitute for the app-level control: a closed ITAR host should
already be physically/logically disconnected. The firewall is the last
independent layer, not the only one.

## Policy

- Loopback interface in/out: allowed.
- Established/related return traffic: allowed.
- TCP to the local model port (default `8080`, matching `calibration.json`
  `"port"`) on `127.0.0.1` / `::1`: allowed.
- **Everything else (DNS, HTTP(S), update checks, telemetry, package fetches):
  DROPPED by default policy.**

Keep `MODEL_PORT` in sync with `deploy/llama-server/calibration.json`.
