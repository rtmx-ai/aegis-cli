package opencode

import (
	"strconv"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// TestRenderConfigDeclaresCtxLimit → REQ-PERF-009: the rendered OpenCode config must declare the model's
// context limit so OpenCode counts tokens against the SAME window aegis serves. Without it OpenCode falls
// back to a small default and compacts (then re-ingests the prompt) at a phantom window — the field bug.
// The declared limit follows cfg.CtxSize (the one resolved value), defaulting to serving.DefaultCtxSize.
func TestRenderConfigDeclaresCtxLimit(t *testing.T) {
	base := config.Config{Endpoint: "http://127.0.0.1:8080", ModelID: "m"}

	// Unset CtxSize → the config still declares a limit, at the one default (never a small fallback).
	out := RenderConfig(base, true)
	if !strings.Contains(out, `"limit"`) || !strings.Contains(out, `"context": `+strconv.Itoa(serving.DefaultCtxSize)) {
		t.Errorf("rendered config must declare limit.context = DefaultCtxSize (%d); got:\n%s", serving.DefaultCtxSize, out)
	}

	// An explicitly resolved CtxSize must flow through verbatim, so OpenCode matches --ctx-size exactly.
	cfg := base
	cfg.CtxSize = 24576
	out = RenderConfig(cfg, true)
	if !strings.Contains(out, `"context": 24576`) {
		t.Errorf("rendered config must declare the resolved cfg.CtxSize (24576); got:\n%s", out)
	}
}
