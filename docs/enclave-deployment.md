# Enclave Deployment — Stage-then-Disconnect

The air-gap transfer procedure (ENCLAVE-002): **build + stage everything on a
connected host, verify it, carry one artifact set across, then run with networking
disabled.** Nothing in the enclave ever fetches from the network — every component
is built from a pinned source and verified by digest/signature before transfer.

## 1. Stage (connected build host)

```bash
./setup.sh --model /path/to/model.gguf
```

`setup.sh` builds the full stack from pinned source (aegis + OpenCode + llama.cpp),
stages + **sha256-verifies** the model GGUF, calibrates serving to the host, and
runs the integration smoke. Pins: `deploy/{opencode/OPENCODE_REF,
llama-server/LLAMA_REF, models/MODEL_REF}`. For a signed, checksummed bundle of the
same artifacts, `make release`.

## 2. Verify before transfer

```bash
make verify-release        # offline detached signature over SHA256SUMS (BUILD-009)
sha256sum -c SHA256SUMS    # every artifact's digest
```

Confirm the model digest matches `deploy/models/MODEL_REF` and the OpenCode/
llama.cpp pins match the intended refs. Provenance is auditable end to end: pinned
source → frozen deps → SBOM → signature → digest.

## 3. Transfer one artifact set

Carry across via approved removable media (the controlled-data transfer process):

- `bin/aegis` (or the platform binary from the release)
- `deploy/opencode/bin/opencode`, `deploy/llama-server/bin/llama-server`
- the staged GGUF + `deploy/llama-server/calibration.json`
- `rtmx` (the intent engine + MCP server)
- the hardened `deploy/opencode/opencode.json`

No source, no package manager, no model registry — only the verified binaries +
model.

## 4. Disconnect + run (closed host)

Bring networking down (default-deny per `deploy/firewall/`), then:

```bash
aegis verify-env                       # egress + traceability status before any run
scripts/verify-airgap.sh -- aegis run "<task>"   # EGRESS=0 across the whole run
aegis                                  # the OpenCode TUI on the local model
aegis loop                             # drain the rtmx backlog unattended
```

The model server (`llama-server`) launches under the calibrated args on loopback
`:8080`; OpenCode is pointed at it via the hardened config; rtmx is the intent
layer. **Egress beyond loopback is a hard failure, by construction** — verified by
`scripts/verify-airgap.sh` (the EGRESS=0 gate) across the whole process group
(aegis + opencode + model + rtmx; ENCLAVE-001).

## 5. Operate

Day-to-day operation (TUI / `run` / `loop` / `status` / pass-throughs) is in
[docs/operator-guide.md](operator-guide.md). Re-staging (model/tool bumps) repeats
§1–4 on the connected host — deliberate, pinned, and re-verified each time.
