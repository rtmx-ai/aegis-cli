package offline

import (
	"strings"
	"testing"
)

// TestHarnessProvisionKeybind → REQ-OC-027: the no-model download keybind must be registered in
// OPENCODE_BASE_MODE so it actually dispatches. OC-026's bind lacked the mode, so it sat below the
// prompt's own ctrl+d (input-delete / app-exit) and never fired — a non-firing bind the original
// patch-assertion guard did not catch.
func TestHarnessProvisionKeybind(t *testing.T) {
	patch := readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
	for _, want := range []string{
		"OPENCODE_BASE_MODE", // the bind must be in the active home-screen mode...
		"startProvision",     // ...and dispatch provisioning
	} {
		if !strings.Contains(patch, want) {
			t.Errorf("the provisioning keybind must be registered in the active mode: missing %q", want)
		}
	}
}
