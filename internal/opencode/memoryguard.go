package opencode

import (
	"fmt"
	"path/filepath"
)

// protectedIntentFiles are the human-authored intent/instruction files aegis's
// machine-written memory must NEVER rewrite (MEM-004). Auto-learned memory that
// edited these would silently override human intent — so it is forbidden by
// construction (cf. the deferred headroom `learn` decision).
var protectedIntentFiles = map[string]bool{
	"CLAUDE.md":      true,
	"AGENTS.md":      true,
	"AGENT.md":       true,
	"GEMINI.md":      true,
	".clinerules":    true,
	".cursorrules":   true,
	".windsurfrules": true,
}

// IsProtectedIntentFile reports whether path is a human-authored intent file that
// machine-written memory must never rewrite (MEM-004). Matched by base name.
func IsProtectedIntentFile(path string) bool {
	return protectedIntentFiles[filepath.Base(path)]
}

// GuardIntentWrite returns an error when path targets a protected intent file, so a
// machine-memory writer cannot clobber human-authored intent (MEM-004).
func GuardIntentWrite(path string) error {
	if IsProtectedIntentFile(path) {
		return fmt.Errorf("aegis: refusing to write machine memory to human intent file %q (MEM-004)", filepath.Base(path))
	}
	return nil
}
