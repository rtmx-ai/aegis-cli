package opencode

import (
	"os"
	"path/filepath"
)

// StagedConfigSeedRelPath is the OpenCode config dir aegis points OPENCODE_CONFIG_DIR
// at (resolved alongside the aegis binary, then relative to cwd). It is pre-seeded so
// OpenCode's bootstrap finds @opencode-ai/plugin already installed and performs NO
// npm install — closing the registry.npmjs.org egress vector that otherwise fires
// (and stalls the run) at startup (OC-010, air-gap). Pointing OPENCODE_CONFIG_DIR
// here also makes it OpenCode's Global.Path.config, so it is the only config-scope
// install target.
const StagedConfigSeedRelPath = "deploy/opencode/config-seed"

// pluginSeedFiles are the minimal files that satisfy OpenCode's bootstrap check
// "is @opencode-ai/plugin installed?" so it skips the npm install: a node_modules
// entry for the package AND a lockfile that lists it (core npm.ts — node_modules
// present + the dep locked => no reify). The package is a stub; OpenCode's plugin
// runtime types are compiled into the binary, so a stub entry is enough to suppress
// the install without changing behavior.
var pluginSeedFiles = map[string]string{
	"package.json":      "{\"name\":\"opencode-config\",\"private\":true,\"dependencies\":{\"@opencode-ai/plugin\":\"*\"}}\n",
	"package-lock.json": "{\"name\":\"opencode-config\",\"lockfileVersion\":3,\"requires\":true,\"packages\":{\"\":{\"dependencies\":{\"@opencode-ai/plugin\":\"*\"}},\"node_modules/@opencode-ai/plugin\":{\"version\":\"0.0.0\"}}}\n",
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
		cands = append(cands, filepath.Join(filepath.Dir(self), "config-seed"))
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
		return absOf(c), true
	}
	return "", false
}
