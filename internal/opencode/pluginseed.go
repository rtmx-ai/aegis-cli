package opencode

import (
	"os"
	"path/filepath"
)

// StagedConfigSeedRelPath is OpenCode's config directory — the `opencode` subdir of
// the XDG config home aegis controls. It is pre-seeded so OpenCode's bootstrap finds
// @opencode-ai/plugin already installed and performs NO npm install, closing the
// registry.npmjs.org egress vector that otherwise fires at startup (OC-010, air-gap).
//
// It MUST be named `opencode` and sit under an XDG base, because OpenCode derives its
// config dir as `xdg-basedir(XDG_CONFIG_HOME) + "opencode"` (Global.Path.config). The
// launch sets XDG_CONFIG_HOME to this dir's parent so that resolves here. (OPENCODE_
// CONFIG_DIR does NOT redirect Global.Path.config — it only overrides a separate
// service accessor — so the bootstrap install would still target ~/.config/opencode
// and reach the registry. That was the OC-010 gap.)
const StagedConfigSeedRelPath = "deploy/opencode/oc-config/opencode"

// pluginSeedFiles are the minimal files that satisfy OpenCode's bootstrap check
// "is @opencode-ai/plugin installed?" so it skips the npm install: a node_modules
// entry for the package AND a lockfile that lists it (core npm.ts — node_modules
// present + the dep locked => no reify). The package is a stub; OpenCode's plugin
// runtime types are compiled into the binary, so a stub entry is enough to suppress
// the install without changing behavior.
// ContextEfficiencyPluginFile is the staged aegis plugin that trims the model context before each
// model call (PERF-004/005). It loads via the rendered config's "plugin" field on the TUI (serve)
// path; the headless --pure path skips plugins by design.
const ContextEfficiencyPluginFile = "aegis-context-efficiency.js"

// contextEfficiencyPlugin trims the model context before each model call (opencode's
// experimental.chat.messages.transform hook), all deterministically — no LLM, no egress: it strips
// stale reasoning parts (PERF-005), bounds oversized tool results (PERF-004), dedupes repeated
// identical tool calls (re-reading the same file), and elides the stale middle of a long observation
// stream keeping the first-N and recent-M (LONGRUN-002). Deterministic pruning runs so opencode's
// expensive LLM compaction triggers later, keeping the prompt cache valid for more turns.
const contextEfficiencyPlugin = `export const ContextEfficiency = async () => ({
  "experimental.chat.messages.transform": async (_input, output) => {
    const MAX = 8000
    const KEEP_FIRST = 2, KEEP_RECENT = 6
    const trunc = (s) => s.slice(0, MAX) + "\n... [aegis: truncated " + (s.length - MAX) + " chars to preserve context]"
    const msgs = (output && output.messages) || []
    // PERF-005 + PERF-004: strip reasoning; bound oversized tool output.
    for (const msg of msgs) {
      if (!msg || !Array.isArray(msg.parts)) continue
      msg.parts = msg.parts.filter((p) => p && p.type !== "reasoning")
      for (const p of msg.parts) {
        if (!p) continue
        const inv = p.toolInvocation
        if (inv && typeof inv.result === "string" && inv.result.length > MAX) inv.result = trunc(inv.result)
        const st = p.state
        if (st && typeof st.output === "string" && st.output.length > MAX) st.output = trunc(st.output)
      }
    }
    // LONGRUN-002 (deterministic pruning): collect every tool observation in order.
    const tools = []
    for (const msg of msgs) {
      if (!msg || !Array.isArray(msg.parts)) continue
      for (const p of msg.parts) {
        if (!p) continue
        const inv = p.toolInvocation
        if (inv && typeof inv.result === "string") {
          tools.push({ key: (inv.toolName || "") + "\x00" + JSON.stringify(inv.args || null), get: () => inv.result, set: (v) => { inv.result = v } })
          continue
        }
        const st = p.state
        if (st && typeof st.output === "string") {
          tools.push({ key: (p.tool || p.toolName || "") + "\x00" + JSON.stringify(st.input || st.args || null), get: () => st.output, set: (v) => { st.output = v } })
        }
      }
    }
    // Dedupe repeated identical tool calls (e.g. re-reading the same file): keep the last, mask earlier.
    const last = new Map()
    tools.forEach((t, i) => last.set(t.key, i))
    tools.forEach((t, i) => { if (last.get(t.key) !== i) t.set("[aegis: superseded by a later identical call]") })
    // Keep the first-N and most-recent-M observations; elide the stale middle of a long run.
    if (tools.length > KEEP_FIRST + KEEP_RECENT) {
      for (let i = KEEP_FIRST; i < tools.length - KEEP_RECENT; i++) {
        const cur = tools[i].get()
        if (cur && cur.indexOf("[aegis:") !== 0) tools[i].set("[aegis: elided stale observation]")
      }
    }
  },
})
export default ContextEfficiency
`

var pluginSeedFiles = map[string]string{
	ContextEfficiencyPluginFile:                     contextEfficiencyPlugin,
	"package.json":                                  "{\"name\":\"opencode-config\",\"private\":true,\"dependencies\":{\"@opencode-ai/plugin\":\"*\"}}\n",
	"package-lock.json":                             "{\"name\":\"opencode-config\",\"lockfileVersion\":3,\"requires\":true,\"packages\":{\"\":{\"dependencies\":{\"@opencode-ai/plugin\":\"*\"}},\"node_modules/@opencode-ai/plugin\":{\"version\":\"0.0.0\"}}}\n",
	"node_modules/@opencode-ai/plugin/package.json": "{\"name\":\"@opencode-ai/plugin\",\"version\":\"0.0.0\",\"type\":\"module\",\"main\":\"index.js\"}\n",
	"node_modules/@opencode-ai/plugin/index.js":     "export {};\n",
}

// stagePluginSeed materializes the plugin seed into dir, idempotently: it writes any
// missing seed file and leaves existing ones untouched.
func stagePluginSeed(dir string) error {
	for rel, content := range pluginSeedFiles {
		p := filepath.Join(dir, filepath.FromSlash(rel))
		if _, err := os.Stat(p); err == nil {
			continue
		}
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			return err
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			return err
		}
	}
	return nil
}

// ConfigSeedDir resolves the staged config-seed directory, materializes the plugin
// seed into it (idempotent), and returns its absolute path. It tries alongside the
// running aegis binary (bundled release) first, then the staged path relative to
// cwd. Returns ok=false only if it cannot stage anywhere, in which case the launch
// omits OPENCODE_CONFIG_DIR and OpenCode falls back to its default config dir.
func ConfigSeedDir() (string, bool) {
	var cands []string
	if self, err := os.Executable(); err == nil {
		cands = append(cands, filepath.Join(filepath.Dir(self), "oc-config", "opencode"))
	}
	for _, d := range LibexecDirs() { // REL-005: package install layout
		cands = append(cands, filepath.Join(d, "oc-config", "opencode"))
	}
	// REL-006: a writable user-cache fallback so a read-only PACKAGED aegis (e.g. installed
	// to /usr where libexec is not user-writable) can materialize the seed at runtime instead
	// of needing it pre-bundled. Comes before the cwd-staged path so a packaged binary never
	// pollutes the user's working directory with deploy/opencode/oc-config.
	if cache, err := os.UserCacheDir(); err == nil {
		cands = append(cands, filepath.Join(cache, "aegis", "oc-config", "opencode"))
	}
	cands = append(cands, StagedConfigSeedRelPath)
	for _, c := range cands {
		if err := stagePluginSeed(c); err != nil {
			continue
		}
		// RUNQ-002: stage the tool-call coaching instruction alongside the plugin
		// seed so the rendered config can reference it (idempotent).
		coaching := filepath.Join(c, toolCoachingFile)
		if _, err := os.Stat(coaching); err != nil {
			if err := os.WriteFile(coaching, []byte(toolCoachingContent), 0o644); err != nil {
				continue
			}
		}
		// PERSONA-001: stage the interactive persona alongside the headless directives.
		interactive := filepath.Join(c, interactiveDirectivesFile)
		if _, err := os.Stat(interactive); err != nil {
			if err := os.WriteFile(interactive, []byte(interactiveDirectivesContent), 0o644); err != nil {
				continue
			}
		}
		return absOf(c), true
	}
	return "", false
}
