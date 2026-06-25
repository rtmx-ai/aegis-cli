# aegis Operator Guide

How to bring up and run aegis — the air-gap-native agentic coding stack (OpenCode
TUI + a local model + rtmx intent). Two phases: **build/stage on a connected
host**, then **install + run in the closed enclave**.

## 1. Build + stage (connected host)

One command builds the full stack from pinned source, stages + verifies the
model, calibrates serving, and smoke-tests the whole stack:

```bash
./setup.sh --model /path/to/model.gguf
```

`setup.sh` (see REL-004) runs: `make ci-full` (aegis + OpenCode + llama-server
from pinned source) → `scripts/stage-model.sh` (sha256-verified GGUF) →
`scripts/bench.sh` (host calibration) → `scripts/integration-smoke.sh`
(full-stack, EGRESS=0). Prerequisites: Go 1.25.x, Bun, a C/C++ toolchain (cmake +
cc). Pins live in `deploy/{opencode/OPENCODE_REF, llama-server/LLAMA_REF,
models/MODEL_REF}`.

Artifacts to carry into the enclave (a signed release bundles these — see
`make release`):

- `./bin/aegis` (or the platform binary from the release)
- `deploy/opencode/bin/opencode`, `deploy/llama-server/bin/llama-server`
- the staged GGUF + `deploy/llama-server/calibration.json`
- `rtmx` on PATH (the intent engine + MCP server)

## 2. Install + run (closed enclave)

Transfer the artifact set, verify the signature, then:

```bash
# verify the release first (offline detached signature over the checksums)
make verify-release            # or: minisign -V -m SHA256SUMS ...

# confirm the environment is closed + traceable before any run
aegis verify-env               # egress + rtmx traceability status

# launch the centerpiece — the hardened OpenCode TUI on the local model
aegis                          # bare command = the TUI

# or drive it headlessly:
aegis run "<prompt>"           # one agent task -> transcript (≡ opencode/ollama run)
aegis loop --once              # drain one rtmx requirement (the orchestration loop)
```

The local model server (llama-server) is launched under the calibrated args by
`internal/serving` (loopback `:8080`); `aegis` renders the hardened OpenCode
config (offline, telemetry/share off, rtmx wired as the MCP intent layer) and
points it at that endpoint.

## 3. Verify the air-gap holds

Egress beyond loopback is a hard failure, by construction:

```bash
scripts/verify-airgap.sh -- aegis run "<prompt>"   # EGRESS=0 across the run
```

The pass-through namespaces expose the full inner tools, hardened:
`aegis rtmx <args>` (intent), `aegis code <args>` (OpenCode), `aegis model <args>`
(the local model server).

## 4. Pins + provenance

Everything is built from pinned source on the connected host, never fetched in
the enclave: OpenCode (`OPENCODE_REF`, tracks the latest upstream **stable** —
`scripts/check-opencode-latest.sh`), llama.cpp (`LLAMA_REF`), and the model
(`MODEL_REF`, name + sha256, verified by `stage-model.sh`). The release is signed
(minisign/GPG, offline detached) with an SBOM + SHA-256 manifest.
