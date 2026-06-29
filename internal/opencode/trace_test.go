package opencode

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestTraceabilityUI → REQ-OC-021: the TUI exposes a /trace traceability view — the live RTM
// (requirement->test matrix, status, completion %) rendered in-session and re-runnable as the
// loop closes requirements. A config command over the bundled rtmx (status + health).
func TestTraceabilityUI(t *testing.T) {
	cfg := config.Config{Endpoint: "http://127.0.0.1:8080"}
	rc := RenderConfig(cfg, true)
	for _, want := range []string{`"trace"`, "rtmx status", "rtmx health", "COMPLETE", "completion percentage", "matrix"} {
		if !strings.Contains(rc, want) {
			t.Errorf("RenderConfig(intent=true) must define the /trace traceability view: missing %q", want)
		}
	}
	if !json.Valid([]byte(rc)) {
		t.Errorf("rendered config with /trace must be valid JSON:\n%s", rc)
	}
}
