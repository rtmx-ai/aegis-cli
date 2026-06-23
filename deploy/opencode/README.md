# deploy/opencode — hardened opencode harness config

`opencode.json` is the default harness, pre-hardened for the closed environment.
Never rely on an operator to remember a phone-home toggle — the offline-safe
posture is baked in here.

## Hardening (every egress vector off by construction)

- `offline: true` — opencode runs in offline mode.
- `share: "disabled"` — no session sharing / upload.
- `autoupdate: false` — no self-update fetch.
- `telemetry: false`, `analytics: false` — no usage phone-home.
- `provider.local.options.baseURL: http://127.0.0.1:8080/v1` — the model is the
  **loopback** OpenAI-compatible endpoint (llama-server / Ollama). No remote
  provider, no API key that implies a remote.

### GUARD-002 — no models.dev hit

opencode normally resolves model metadata from `models.dev`. With an explicit
local provider + `offline: true` the harness must **not** contact `models.dev`
(or any other host) on launch. That is exactly what `GUARD-002` verifies and what
`scripts/verify-airgap.sh` proves: launching opencode with this config produces
zero non-loopback packets.

## rtmx MCP server (the dev-loop foundation)

The `mcp.rtmx` block registers rtmx as a **local stdio extension**
(`rtmx mcp-server --stdio`). rtmx is the requirements engine that drives the loop;
exposing it over stdio (not a socket) keeps it in-process and egress-free.
