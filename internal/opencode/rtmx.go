package opencode

import (
	"os"
	"path/filepath"
)

// StagedRtmxRelPath is where the air-gap-staged rtmx intent binary lives for bundling
// (scripts/stage-rtmx.sh, OC-019). aegis ships rtmx in the package so the TUI's rtmx MCP
// (next/claim/verify/set_status) works out of the box — no operator install — and its directory
// is prepended to the launch PATH so the MCP command `["rtmx", ...]` resolves the bundled binary.
const StagedRtmxRelPath = "deploy/rtmx/bin/rtmx"

// ResolveRtmx finds the bundled rtmx: alongside the running aegis binary (bundled release), the
// package libexec (REL-005), then the staged path relative to cwd. It does NOT fall back to PATH
// (returning ok=false instead) so a hand-installed rtmx is never silently preferred over the
// bundled one; when no bundled rtmx is present the launch PATH is left to the inherited
// environment (a dev host with rtmx on PATH still works).
func ResolveRtmx() (string, bool) {
	var cands []string
	if self, err := os.Executable(); err == nil {
		dir := filepath.Dir(self)
		cands = append(cands, filepath.Join(dir, "rtmx"), filepath.Join(dir, StagedRtmxRelPath))
	}
	for _, d := range LibexecDirs() { // REL-005: package install layout
		cands = append(cands, filepath.Join(d, "rtmx"))
	}
	cands = append(cands, StagedRtmxRelPath)
	for _, c := range cands {
		if isExecutable(c) {
			return absOf(c), true
		}
	}
	return "", false
}
