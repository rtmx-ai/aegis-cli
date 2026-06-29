package opencode

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestRtmxSlashCommand → REQ-OC-020: the TUI exposes a /rtmx command that runs the bundled rtmx
// intent engine (next/claim/verify/status/health/backlog) with output inline. It is a config
// command (prompt template expanded with $ARGUMENTS), wired only when intent is on.
func TestRtmxSlashCommand(t *testing.T) {
	cfg := config.Config{Endpoint: "http://127.0.0.1:8080"}
	rc := RenderConfig(cfg, true)
	for _, want := range []string{`"command"`, `"rtmx"`, "rtmx $ARGUMENTS", "status", "backlog", "claim"} {
		if !strings.Contains(rc, want) {
			t.Errorf("RenderConfig(intent=true) must define the /rtmx command: missing %q", want)
		}
	}
	if !json.Valid([]byte(rc)) {
		t.Errorf("rendered config with the /rtmx command must be valid JSON:\n%s", rc)
	}
	if strings.Contains(RenderConfig(cfg, false), `"command"`) {
		t.Error("RenderConfig(intent=false) must omit the /rtmx command (control parity with the MCP)")
	}
}
