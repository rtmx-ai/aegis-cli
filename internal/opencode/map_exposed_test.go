package opencode

import (
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestMapExposed → REQ-INDEX-001-P05: the repo map is exposed to the harness as the
// /map opencode command (shelling `aegis map`, no fork), and omitted without intent.
func TestMapExposed(t *testing.T) {
	rc := RenderConfig(config.Config{Endpoint: "http://127.0.0.1:8080"}, true)
	if !strings.Contains(rc, `"map"`) || !strings.Contains(rc, "aegis map") {
		t.Errorf("RenderConfig(intent) must expose the /map command; got:\n%s", rc)
	}
	if strings.Contains(RenderConfig(config.Config{Endpoint: "x"}, false), "aegis map") {
		t.Error("RenderConfig(intent=false) must omit /map (control parity with the other commands)")
	}
}
