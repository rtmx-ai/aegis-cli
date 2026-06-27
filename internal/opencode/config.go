package opencode

import (
	"encoding/json"
	"fmt"
	"path/filepath"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// toolCoachingFile is the instruction file aegis stages into the OpenCode config
// dir and wires into the rendered config's `instructions` (RUNQ-002). It coaxes
// small local models to emit real tool calls instead of prose — the failure mode
// observed in the bake-off (a weak model narrates an edit rather than calling the
// edit tool). OpenCode appends `instructions` files to the agent's system prompt.
const toolCoachingFile = "tool-coaching.md"

// toolCoachingContent is the system-prompt coaching. Kept terse and imperative —
// small models follow short, concrete directives better than prose.
const toolCoachingContent = `# Operating directives (aegis headless run)

You are in a headless, air-gapped coding session. Every change MUST be made by
calling a tool — never print code, diffs, file contents, or "here is what I would
do" as prose.

- Edit an existing file with the **edit** tool; create one with **write**; run
  commands or tests with **bash**; find code with **grep**/**glob**/**read**.
- Inspect before you change: read the target file, then make one focused edit.
- A reply that contains only prose and no tool call is a failure — call a tool.
- When the task is done and its tests pass, stop.
`

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
// When intent is true, rtmx is wired as the MCP intent layer; when false it is
// omitted — the "control" condition for intent-bench (BENCH-004), so a run reports
// zero intent-tool tokens.
func RenderConfig(cfg config.Config, intent bool) string {
	model := cfg.ModelID
	if model == "" {
		model = "local-moe"
	}
	baseURL := cfg.Endpoint + "/v1"
	mcp := ""
	if intent {
		mcp = `,
  "mcp": {
    "rtmx": { "type": "local", "enabled": true, "command": ["rtmx", "mcp-server", "--stdio"] }
  }`
	}
	// RUNQ-002: append the tool-call coaching instruction (staged into the config
	// seed dir, OC-010) to the agent's system prompt so small models call tools
	// instead of emitting prose. Absolute path — OpenCode resolves it directly.
	instructions := ""
	if seed, ok := ConfigSeedDir(); ok {
		instructions = fmt.Sprintf(`,
  "instructions": [%q]`, filepath.Join(seed, toolCoachingFile))
	}
	// SERVE-020 (per-model tuning) + RUNQ-003 (step/output limits) on the build agent:
	// tuning shapes sampling so the model tool-calls reliably; the limits bound a
	// capable-but-rambling model so it completes instead of running away.
	agent := renderAgent(cfg)
	// Classic opencode.json schema: provider + model (+ optional mcp). Air-gap
	// hardening is enforced via env markers (OPENCODE_TELEMETRY/AUTOUPDATE/
	// DISABLE_SHARE), `opencode run --pure` (no external plugins), and the egress
	// gate — the classic schema rejects unknown top-level keys like offline/telemetry.
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
  "model": %q%s%s%s
}`, baseURL, model, model, "local/"+model, mcp, instructions, agent)
}

// renderAgent emits the OpenCode `agent.build` block: per-model tuning (SERVE-020) plus the
// run's step/output limits (RUNQ-003), or "" when none is set. Built via encoding/json so
// floats format correctly. temperature/top_p/steps are agent fields; the Ollama extensions
// (top_k/min_p/repeat_penalty/num_ctx/think/num_predict) ride `options` (forwarded
// best-effort). `steps` is opencode-enforced (the reliable loop bound).
func renderAgent(cfg config.Config) string {
	build := map[string]any{}
	opts := map[string]any{}
	if t := cfg.Tuning; t != nil {
		if t.Temperature != nil {
			build["temperature"] = *t.Temperature
		}
		if t.TopP != nil {
			build["top_p"] = *t.TopP
		}
		if t.TopK != nil {
			opts["top_k"] = *t.TopK
		}
		if t.MinP != nil {
			opts["min_p"] = *t.MinP
		}
		if t.RepeatPenalty != nil {
			opts["repeat_penalty"] = *t.RepeatPenalty
		}
		if t.NumCtx != nil {
			opts["num_ctx"] = *t.NumCtx
		}
		if t.Think != nil {
			opts["think"] = *t.Think
		}
	}
	// RUNQ-003 run-policy limits.
	if cfg.MaxSteps > 0 {
		build["steps"] = cfg.MaxSteps
	}
	if cfg.MaxOutputTokens > 0 {
		opts["num_predict"] = cfg.MaxOutputTokens
	}
	if len(opts) > 0 {
		build["options"] = opts
	}
	if len(build) == 0 {
		return ""
	}
	b, err := json.Marshal(map[string]any{"build": build})
	if err != nil {
		return ""
	}
	return ",\n  \"agent\": " + string(b)
}
