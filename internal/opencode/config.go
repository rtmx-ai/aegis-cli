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

// interactiveDirectivesFile/Content is the PERSONA-001 system prompt for the interactive TUI: the same
// action-bias as the headless directives, but tuned for a live session — proactive, thorough, curious,
// persevering — rather than do-the-minimum-and-stop. Terse + imperative (small models follow short,
// concrete directives best). The first aegis persona; expected to evolve toward frontier quality.
const interactiveDirectivesFile = "interactive-directives.md"

const interactiveDirectivesContent = `# Operating directives

You are an aegis coding agent working directly in a live repository. Act, don't
describe — and see the work through.

- Make every change with a tool: **edit**/**write** to change files, **bash** to run
  commands and tests, **grep**/**glob**/**read** to find and inspect code. Never print
  code or diffs as prose; a reply with no tool call while work remains is a failure.
- Investigate before you act. Read the real code and search the repo until you
  understand how it works, not just enough to start — precision follows from looking first.
- Be curious about the true cause. When something is off, trace it to its root and
  confirm your theory with evidence before changing anything.
- Carry each task all the way through: the follow-on edits it implies, the test that
  proves it, the obvious next step. Don't stop at the first plausible answer — verify it,
  then keep going until the job is actually done.
- When a detail is ambiguous, choose a sound default and proceed; state what you
  assumed. Keep momentum instead of stalling on a question you can answer yourself.
- Close with a brief, concrete summary: what you changed, what you ran, and what it proved.
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
	command := ""
	if intent {
		mcp = `,
  "mcp": {
    "rtmx": { "type": "local", "enabled": true, "command": ["rtmx", "mcp-server", "--stdio"] }
  }`
		// OC-020: a /rtmx slash command in the TUI. It runs the bundled rtmx intent engine
		// (OC-019) and renders its output in-session — so the operator drives the intent loop
		// (next/claim/verify/status/health/backlog) without leaving the TUI. $ARGUMENTS is the
		// subcommand line (opencode commands are prompt templates expanded with $ARGUMENTS).
		command = `,
  "command": {
    "rtmx": {
      "description": "rtmx intent: next/claim/verify/status/health/backlog",
      "template": "Run the bundled rtmx intent engine: execute the shell command 'rtmx $ARGUMENTS' and show its full output verbatim, then stop — do not edit files. rtmx subcommands: next, claim <id>, verify, status, health, backlog. If no arguments were given, run 'rtmx status'."
    },
    "trace": {
      "description": "Requirements traceability — status, completion %, the requirement->test matrix",
      "template": "Show the requirements traceability state, then stop — do not edit files. Run the shell commands 'rtmx status' (the requirement->test matrix: each requirement's status COMPLETE/PARTIAL/MISSING and its mapped test) and 'rtmx health' (completion %, orphaned requirements/tests, reciprocity, the gates) and show their output verbatim. Lead with the headline: total requirements and the COMPLETE / PARTIAL / MISSING breakdown with the completion percentage. This is the live intent state; re-run /trace after closing a requirement to see it update."
    },
    "map": {
      "description": "Repo map — a ranked, token-budgeted skeleton of the codebase (real symbols to call)",
      "template": "Run the shell command 'aegis map $ARGUMENTS' and show its full output verbatim, then stop — do not edit files. It is a ranked, token-budgeted skeleton (definition signatures) of the repository built by static analysis (no model); use it to locate and call real symbols instead of loading whole files. Pass identifiers or paths as arguments to focus the map on the current task."
    },
    "licenses": {
      "description": "Third-party software notices — the open-source components aegis is built on",
      "template": "Show aegis's third-party software disclosures verbatim, exactly as written below, then stop — do not edit files or add commentary:\n\naegis is built on and bundles these open-source components, with gratitude:\n  • OpenCode — the agentic TUI/harness — MIT License, Copyright (c) 2025 opencode\n  • llama.cpp — local model serving — MIT License, Copyright (c) 2023-2024 The ggml authors\n  • ripgrep — fast code search — MIT License / The Unlicense, Copyright (c) 2016 Andrew Gallant\n  • rtmx — the requirements / intent engine — (c) ioTACTICAL\n  • Gemma — the default local model weights — (c) Google, used under the Gemma Terms of Use\n\nThe full license texts ship in THIRD-PARTY-NOTICES.md alongside the aegis binary."
    }
  }`
	}
	// RUNQ-002: append the tool-call coaching instruction (staged into the config
	// seed dir, OC-010) to the agent's system prompt so small models call tools
	// instead of emitting prose. Absolute path — OpenCode resolves it directly.
	instructions := ""
	if seed, ok := ConfigSeedDir(); ok {
		directives := toolCoachingFile
		if cfg.Interactive { // PERSONA-001: the proactive persona for the interactive TUI
			directives = interactiveDirectivesFile
		}
		list := fmt.Sprintf("%q", filepath.Join(seed, directives))
		// INDEX-001: auto-inject the repo map (codebase skeleton) as context on the
		// interactive path, so the model has real symbols without invoking /map.
		if cfg.Interactive {
			if rm := filepath.Join(seed, RepoMapFile); fileExists(rm) {
				list += ", " + fmt.Sprintf("%q", rm)
			}
		}
		instructions = ",\n  \"instructions\": [" + list + "]"
	}
	// PERF-004/005: load the staged context-efficiency plugin (strip reasoning + bound tool output) so
	// the context stays lean — only the TUI (serve) path loads it; the headless --pure path skips it.
	plugin := ""
	if seed, ok := ConfigSeedDir(); ok {
		plugin = fmt.Sprintf(`,
  "plugin": [%q]`, filepath.Join(seed, ContextEfficiencyPluginFile))
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
  "model": %q%s%s%s%s%s
}`, baseURL, model, model, "local/"+model, mcp, command, instructions, plugin, agent)
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
