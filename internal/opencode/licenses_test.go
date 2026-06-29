package opencode

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestLicensesCommand → OC-014/OC-016: the TUI exposes /licenses showing the third-party software
// notices (the open-source components aegis bundles), and the multiline template keeps the config
// valid JSON.
func TestLicensesCommand(t *testing.T) {
	cfg := config.Config{Endpoint: "http://127.0.0.1:8080"}
	rc := RenderConfig(cfg, true)
	for _, want := range []string{`"licenses"`, "OpenCode", "llama.cpp", "ripgrep", "rtmx", "Gemma", "MIT", "THIRD-PARTY-NOTICES"} {
		if !strings.Contains(rc, want) {
			t.Errorf("RenderConfig(intent=true) must define /licenses with the notices: missing %q", want)
		}
	}
	if !json.Valid([]byte(rc)) {
		t.Errorf("config with /licenses (multiline template) must be valid JSON:\n%s", rc)
	}
}
