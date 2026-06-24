package opencode

import (
	"context"
	"os"
	"path/filepath"
	"testing"

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
	res, err := RunHeadless(context.Background(), bin, config.Default(), dir, "phi4-mini", "do it")
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
