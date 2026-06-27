package opencode

import (
	"os"
	"path/filepath"
)

// LibexecDirs returns the directories to search for aegis's bundled helpers (the OpenCode
// binary, ripgrep, the config-seed, llama-server) in a PACKAGE install layout, highest
// priority first:
//
//   - $AEGIS_LIBEXEC (explicit override)
//   - <prefix>/lib/aegis      (Debian: /usr/bin/aegis -> /usr/lib/aegis)
//   - <prefix>/libexec/aegis
//   - <prefix>/libexec        (Homebrew: <cellar>/bin/aegis -> <cellar>/libexec)
//
// where <prefix> is derived from the running aegis binary's location. This lets a packaged
// `aegis` on PATH find its helpers without the alongside-exe / cwd-relative search that only
// works from a bundle dir or the source tree (REL-005). Callers append the helper's name
// (e.g. "opencode", "rg", "llama-server", or "oc-config/opencode") to each dir.
func LibexecDirs() []string {
	var dirs []string
	if d := os.Getenv("AEGIS_LIBEXEC"); d != "" {
		dirs = append(dirs, d)
	}
	if self, err := os.Executable(); err == nil {
		prefix := filepath.Dir(filepath.Dir(self)) // <prefix>/bin/aegis -> <prefix>
		dirs = append(dirs,
			filepath.Join(prefix, "lib", "aegis"),
			filepath.Join(prefix, "libexec", "aegis"),
			filepath.Join(prefix, "libexec"),
		)
	}
	return dirs
}
