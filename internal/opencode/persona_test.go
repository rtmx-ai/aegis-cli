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

// TestInteractivePersonaContent → PERSONA-001/002: the interactive persona carries the action +
// thoroughness + curiosity + perseverance directives that distinguish it from the terse headless one,
// while the headless one keeps its stop-when-done bound.
func TestInteractivePersonaContent(t *testing.T) {
	for _, want := range []string{
		"Act with tools",     // action bias (on tasks)
		"read the real code", // thoroughness / precision
		"true root",          // curiosity
		"Carry each task",    // perseverance
		"concrete summary",   // substantive close
	} {
		if !strings.Contains(interactiveDirectivesContent, want) {
			t.Errorf("interactive persona missing %q", want)
		}
	}
	if !strings.Contains(toolCoachingContent, "stop") {
		t.Error("headless directives must keep the stop-when-done bound")
	}
}

// TestInteractivePersonaDepth → PERSONA-002: the interactive persona must be Q&A-capable, not action-only.
// The v1.9.0 field report was shallow answers to questions — the old persona treated "no tool call" as a
// failure even when the user asked a *question*. The revised persona explicitly branches: act on
// build/fix/change requests, but answer explain/analyze/plan requests in depth with specifics. This test
// guards that the depth branch exists and that a one-line reply to a real question is called out as wrong.
func TestInteractivePersonaDepth(t *testing.T) {
	// The Q&A / depth branch must be present and distinct from the action branch.
	for _, want := range []string{
		"explain, analyze, plan",                           // the question-handling branch header
		"Answer thoroughly",                                // depth directive for questions
		"to accomplish the work",                           // explains the path, not just what exists
		"a one-line reply to a real question is a failure", // shallow answers are wrong
	} {
		if !strings.Contains(interactiveDirectivesContent, want) {
			t.Errorf("interactive persona missing the depth directive %q — questions must get rich answers", want)
		}
	}
	// The action branch must still exist — depth must not cost the action bias on real tasks.
	for _, want := range []string{"do", "make a change", "Act with tools"} {
		if !strings.Contains(interactiveDirectivesContent, want) {
			t.Errorf("interactive persona lost its action directive %q — tasks must still act with tools", want)
		}
	}
}
