package opencode

import (
	"os"
	"path/filepath"
	"strings"
)

// StagedRipgrepRelPath is where the air-gap-staged ripgrep binary lives, alongside
// the staged OpenCode binary (scripts/stage-ripgrep.sh, OC-009). OpenCode's grep
// tool resolves `rg` from PATH and otherwise DOWNLOADS ripgrep from github at
// bootstrap — a non-loopback egress that also wedges the run offline. Staging our
// own rg and putting its directory on the launch PATH keeps that closed.
const StagedRipgrepRelPath = "deploy/opencode/bin/rg"

// ResolveRipgrep finds the bundled, air-gap-staged ripgrep: alongside the running
// aegis binary (bundled release), then the staged path relative to cwd. Unlike the
// OpenCode binary it does NOT fall back to PATH — the whole point is to bundle our
// own rg so the launch never reaches the network for one. Returns ok=false when no
// staged rg is present (the launch PATH is then left to the inherited environment).
func ResolveRipgrep() (string, bool) {
	var cands []string
	if self, err := os.Executable(); err == nil {
		dir := filepath.Dir(self)
		cands = append(cands, filepath.Join(dir, "rg"), filepath.Join(dir, StagedRipgrepRelPath))
	}
	for _, d := range LibexecDirs() { // REL-005: package install layout
		cands = append(cands, filepath.Join(d, "rg"))
	}
	cands = append(cands, StagedRipgrepRelPath)
	for _, c := range cands {
		if isExecutable(c) {
			return absOf(c), true
		}
	}
	return "", false
}

// hardenedPath returns PATH with the bundled helpers' directories prepended: the staged ripgrep
// (so OpenCode's which("rg") resolves it instead of fetching from github, OC-009) and the bundled
// rtmx (so the TUI's rtmx MCP command resolves the bundled intent engine, OC-019). It returns ""
// when neither is bundled, leaving PATH to the inherited environment rather than overriding it.
func hardenedPath() string {
	var dirs []string
	if rg, ok := ResolveRipgrep(); ok {
		dirs = append(dirs, filepath.Dir(rg))
	}
	if rtmx, ok := ResolveRtmx(); ok {
		if d := filepath.Dir(rtmx); !contains(dirs, d) {
			dirs = append(dirs, d)
		}
	}
	if len(dirs) == 0 {
		return ""
	}
	if orig := os.Getenv("PATH"); orig != "" {
		dirs = append(dirs, orig)
	}
	return strings.Join(dirs, string(os.PathListSeparator))
}

func contains(ss []string, s string) bool {
	for _, x := range ss {
		if x == s {
			return true
		}
	}
	return false
}
