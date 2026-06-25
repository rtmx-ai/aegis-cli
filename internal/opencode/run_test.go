package opencode

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestRunHeadless → REQ-BENCH-001: drive `opencode run --format json` and parse
// the event stream into a transcript with usage. A fake opencode emits canned
// events so the parser is exercised without a real model.
func TestRunHeadless(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "opencode")
	script := "#!/bin/sh\n" +
		"printf '%s\\n' '{\"type\":\"step_start\",\"part\":{}}'\n" +
		"printf '%s\\n' '{\"type\":\"text\",\"part\":{\"text\":\"hello world\"}}'\n" +
		"printf '%s\\n' '{\"type\":\"step_finish\",\"part\":{\"reason\":\"stop\",\"tokens\":{\"total\":10,\"input\":3,\"output\":7}}}'\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	res, err := RunHeadless(context.Background(), bin, config.Default(), dir, "phi4-mini", "do it", true)
	if err != nil {
		t.Fatal(err)
	}
	if len(res.Messages) != 1 {
		t.Fatalf("want 1 transcript message, got %d", len(res.Messages))
	}
	m := res.Messages[0]
	if m.Text != "hello world" || m.Finish != "stop" || m.Tokens.Output != 7 {
		t.Errorf("parsed event stream wrong: %+v", m)
	}
}

// TestRunHeadlessTimeout → REQ-RUNQ-001: a run that exceeds its wall-clock budget
// is aborted and returns the partial transcript (TimedOut), not a hard error.
func TestRunHeadlessTimeout(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "opencode")
	// Emits a partial event, then hangs past the deadline.
	script := "#!/bin/sh\n" +
		"printf '%s\\n' '{\"type\":\"text\",\"part\":{\"text\":\"partial\"}}'\n" +
		"sleep 30\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 400*time.Millisecond)
	defer cancel()
	res, err := RunHeadless(ctx, bin, config.Default(), dir, "phi4-mini", "do it", true)
	if err != nil {
		t.Fatalf("timeout must not be a hard error: %v", err)
	}
	if !res.TimedOut {
		t.Error("result must be marked TimedOut")
	}
	if len(res.Messages) != 1 || res.Messages[0].Text != "partial" {
		t.Errorf("partial transcript must be returned, got %+v", res.Messages)
	}
}

// TestRenderConfigControl → REQ-BENCH-004: with intent off (control condition) the
// rendered config OMITS the rtmx MCP, so a run reports zero intent-tool tokens.
func TestRenderConfigControl(t *testing.T) {
	cfg := config.Default()
	treatment := RenderConfig(cfg, true)
	control := RenderConfig(cfg, false)
	if !strings.Contains(treatment, "rtmx") {
		t.Error("treatment config must wire rtmx")
	}
	if strings.Contains(control, "rtmx") || strings.Contains(control, "mcp") {
		t.Errorf("control config must omit rtmx/mcp:\n%s", control)
	}
}
