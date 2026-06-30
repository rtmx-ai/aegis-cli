package opencode

import (
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestModeAwareDirectives → PERSONA-001: an interactive TUI session gets the proactive persona; a
// headless run gets the tight stop-when-done directives. The discriminator is cfg.Interactive.
func TestModeAwareDirectives(t *testing.T) {
	if _, ok := ConfigSeedDir(); !ok {
		t.Skip("config seed dir unavailable in this environment")
	}
	base := config.Config{Endpoint: "http://127.0.0.1:8080", ModelID: "m"}

	inter := base
	inter.Interactive = true
	out := RenderConfig(inter, true)
	if !strings.Contains(out, interactiveDirectivesFile) {
		t.Errorf("interactive config must reference %q; got:\n%s", interactiveDirectivesFile, out)
	}
	if strings.Contains(out, toolCoachingFile) {
		t.Errorf("interactive config must NOT reference the headless directives %q", toolCoachingFile)
	}

	head := base // Interactive defaults false
	out = RenderConfig(head, false)
	if !strings.Contains(out, toolCoachingFile) {
		t.Errorf("headless config must reference %q", toolCoachingFile)
	}
	if strings.Contains(out, interactiveDirectivesFile) {
		t.Errorf("headless config must NOT reference the interactive persona %q", interactiveDirectivesFile)
	}
}

// TestInteractivePersonaContent → PERSONA-001: the interactive persona carries the action + thoroughness
// + curiosity + perseverance directives that distinguish it from the terse headless one, while the
// headless one keeps its stop-when-done bound.
func TestInteractivePersonaContent(t *testing.T) {
	for _, want := range []string{
		"Act, don't",              // action bias
		"Investigate before",      // thoroughness / precision
		"true cause",              // curiosity
		"all the way through",     // perseverance
		"brief, concrete summary", // substantive close
	} {
		if !strings.Contains(interactiveDirectivesContent, want) {
			t.Errorf("interactive persona missing %q", want)
		}
	}
	if !strings.Contains(toolCoachingContent, "stop") {
		t.Error("headless directives must keep the stop-when-done bound")
	}
}
