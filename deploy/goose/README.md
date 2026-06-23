# deploy/goose — hardened Goose harness config

Goose is the MCP-native bake-off contender against opencode (`CLAUDE.md` §2).
Both expose OpenAI-compatible + MCP; decide by metrics (`skills/metrics-eval`),
not assumption. `config.yaml` is pre-hardened for the closed environment.

## Hardening (air-gap posture)

- **Model = loopback only.** `OPENAI_HOST: http://127.0.0.1:8080` — the
  OpenAI-compatible local endpoint (llama-server / Ollama). No remote provider.
- **Local extensions only.** `extensions` contains a single **stdio** MCP server
  (rtmx). No SSE/remote extensions — those are egress vectors and are disallowed.
- **Telemetry off / update-check off.** `GOOSE_TELEMETRY_ENABLED: false` and
  `GOOSE_DISABLE_UPDATE_CHECK: true` close the phone-home paths.

## rtmx MCP server (the dev-loop foundation)

The `extensions.rtmx` block registers rtmx as a **local stdio** MCP server
(`rtmx mcp-server --stdio`) — the requirements engine that drives the loop, kept
in-process and egress-free over stdio.

## Usage

Place `config.yaml` at Goose's config path (e.g. `~/.config/goose/config.yaml`)
or point `GOOSE_CONFIG` at it. Then `scripts/verify-airgap.sh -- goose <cmd>`
proves zero non-loopback egress on launch, same gate as opencode.
