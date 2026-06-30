package offline

import (
	"strings"
	"testing"
)

// TestHarnessAutoProvision → REQ-OC-034: the in-TUI no-model screen auto-starts provisioning, renders
// a live progress bar, and cancels via Ctrl+G — asserted through the rebrand patch (the interactive
// TUI behavior can't be exercised headlessly).
func TestHarnessAutoProvision(t *testing.T) {
	patch := readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
	for _, want := range []string{
		"startProvision()",            // the auto-start call on mount
		"provBar",                     // the live progress bar render
		"setProvPct",                  // percent parsed from the streamed progress
		"Download / cancel the model", // Ctrl+G start/cancel toggle
	} {
		if !strings.Contains(patch, want) {
			t.Errorf("OC-034 in-TUI auto-provision missing %q in the rebrand patch", want)
		}
	}
}
