package offline

import (
	"strings"
	"testing"
)

// TestOpenCodeAttribution → REQ-OC-016: rebranding OpenCode is lawful only if its MIT license +
// copyright travel with the distribution. THIRD-PARTY-NOTICES.md retains them, and the bundle +
// .deb builders ship the notices.
func TestOpenCodeAttribution(t *testing.T) {
	notices := readRepoFile(t, "THIRD-PARTY-NOTICES.md")
	for _, want := range []string{
		"OpenCode", "anomalyco/opencode", "MIT License",
		"Copyright (c) 2025 opencode", "Permission is hereby granted",
	} {
		if !strings.Contains(notices, want) {
			t.Errorf("THIRD-PARTY-NOTICES.md must retain OpenCode's MIT attribution: missing %q", want)
		}
	}
	for _, f := range []string{"scripts/build-bundle.sh", "scripts/build-deb.sh"} {
		if !strings.Contains(readRepoFile(t, f), "THIRD-PARTY-NOTICES.md") {
			t.Errorf("%s must ship THIRD-PARTY-NOTICES.md in the distribution", f)
		}
	}
}
