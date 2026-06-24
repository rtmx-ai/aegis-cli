package opencode

import (
	"fmt"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// RenderConfig produces the air-gap-hardened OpenCode config, with the operator's
// loopback endpoint + model substituted in, so the launched OpenCode actually
// targets the configured local model (OC-006). OpenCode 2.0 honors this via the
// OPENCODE_CONFIG_CONTENT env var (packages/core/src/flag/flag.ts), so aegis
// renders it at launch rather than shipping a static endpoint.
//
// The shape matches the documented provider config for the pinned OpenCode
// (deploy/opencode/opencode.json is the reference template): an openai-compatible
// loopback provider, offline + telemetry/share/analytics off, and rtmx as the MCP
// intent layer.
func RenderConfig(cfg config.Config) string {
	model := cfg.ModelID
	if model == "" {
		model = "local-moe"
	}
	baseURL := cfg.Endpoint + "/v1"
	// Classic opencode.json schema: provider + model + mcp. Air-gap hardening is
	// enforced via env markers (OPENCODE_TELEMETRY/AUTOUPDATE/DISABLE_SHARE),
	// `opencode run --pure` (no external plugins), and the egress gate — the
	// classic schema rejects unknown top-level keys like offline/telemetry.
	return fmt.Sprintf(`{
  "$schema": "https://opencode.ai/config.json",
  "share": "disabled",
  "autoupdate": false,
  "provider": {
    "local": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": %q, "apiKey": "not-needed-loopback" },
      "models": { %q: { "name": %q } }
    }
  },
  "model": %q,
  "mcp": {
    "rtmx": { "type": "local", "enabled": true, "command": ["rtmx", "mcp-server", "--stdio"] }
  }
}`, baseURL, model, model, "local/"+model)
}
