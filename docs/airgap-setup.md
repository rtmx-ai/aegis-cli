# Air-Gap Setup Guide — aegis-cli

How to stand up aegis-cli in a closed, ITAR-suitable enclave so that the stack
runs with **zero network egress beyond loopback**. This is the engineering
procedure; final US-person handling, device sourcing, and Technology Control
Plan sign-off are the security/export-control authority's call (see
`docs/hardware-purchase-spec.md`).

## The non-negotiable

No component aegis-cli ships or writes may make a network call other than
loopback to the local model endpoint. Egress is a build-failing condition, not a
warning. See `skills/airgap-hygiene` and `CLAUDE.md` §1.

## 1. Stage everything before disconnecting

All fetching happens once, on a connected build host, then crosses into the
enclave on controlled media:

- The `aegis` static binary (`make build` — std-lib only, no third-party deps,
  so the offline build needs no module fetch).
- The `rtmx` binary (the requirements + closed-loop verification engine).
- The harness (opencode or Goose) and the local model GGUF.
- llama.cpp built **from source** for the target (Metal on macOS, CUDA on
  NVIDIA, ROCm/Vulkan on Strix Halo) — no telemetry build.

Verify the offline build before transfer:

```bash
GOPROXY=off GOFLAGS=-mod=mod go build ./...   # must succeed with no network
```

## 2. Default-deny egress at the host

Defense in depth behind the app-level guarantee. Apply the firewall ruleset
(loopback + the local model port only):

```bash
sudo nft -f deploy/firewall/aegis.nft        # nftables, or
sudo deploy/firewall/aegis-iptables.sh       # iptables
```

## 3. Harden the serving + harness configs

The shipped configs are already offline-safe (egress-capable settings default
off). Use them:

- `deploy/llama-server/` — production serving, built from source, no telemetry.
- `deploy/ollama/` — spike serving, localhost-bound, update-check off.
- `deploy/opencode/opencode.json` — offline on; share/telemetry/autoupdate off;
  model pointed at the loopback endpoint; rtmx registered as a local stdio MCP
  server (no `models.dev` hit — REQ-GUARD-002).
- `deploy/goose/config.yaml` — local extensions only, telemetry off.

## 4. Calibrate to the host

```bash
scripts/bench.sh --model /models/<your-model>.gguf
```

Auto-detects the target (`linux-cpu` vs `darwin-metal`), sweeps thread/batch,
and writes `deploy/llama-server/calibration.json`. An uncalibrated launch is a
hard error. See `skills/serving-calibration`.

## 5. Prove the enclave is closed

This is the gate that must pass before any controlled data is processed:

```bash
aegis verify-env                              # egress + traceability status
scripts/verify-airgap.sh -- aegis run --once  # EGRESS=0 gate around a real run
make ci                                        # full pipeline incl. the egress gate
```

`verify-airgap.sh` runs the command inside a network namespace with only
loopback (`unshare -rn`) so any genuine egress attempt fails at the kernel and
fails the build. On a host where unprivileged namespaces are restricted it falls
back to socket capture; set `AIRGAP_STRICT=1` to make any non-fail-closed branch
a hard failure (CI sets this). Any non-loopback egress fails the run — this is
the ITAR control expressed as a test.

## 6. MCP dev loop

The rtmx MCP server (`.mcp.json` → `rtmx mcp-server --stdio`) is the dev-loop
entry point: it exposes `next`/`claim`/`release`/`verify`/`status`/`health` as
tools the harness drives. It is stdio/loopback only — no network surface.

## Ongoing posture

- Audit log is append-only and stays in-enclave.
- Controlled work never lands in the public `rtmx-ai/aegis-cli` repo — it lives
  only on the internal in-enclave remote. aegis-cli the tool is open source; the
  mission work it drives is not.
- Re-run `make ci` (egress gate + TRACE + ACR) on every change. If a dependency
  is ever added, it must be vendored and the offline build must still pass.
