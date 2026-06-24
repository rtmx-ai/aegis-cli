package bench

import (
	"bufio"
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/opencode"
)

// TestTranscriptExport → REQ-BENCH-002: NDJSON with per-message usage + a final
// result record carrying totals (intent-bench parseable).
func TestTranscriptExport(t *testing.T) {
	msgs := []opencode.TranscriptMessage{
		{Role: "user", Text: "build X"},
		{Role: "assistant", Text: "done", Finish: "stop", Tokens: opencode.Tokens{Input: 100, Output: 42, Total: 142}},
	}
	var buf bytes.Buffer
	if err := WriteTranscript(&buf, msgs); err != nil {
		t.Fatal(err)
	}
	var lines []map[string]any
	sc := bufio.NewScanner(&buf)
	for sc.Scan() {
		if strings.TrimSpace(sc.Text()) == "" {
			continue
		}
		var m map[string]any
		if err := json.Unmarshal(sc.Bytes(), &m); err != nil {
			t.Fatalf("each line must be valid JSON: %v", err)
		}
		lines = append(lines, m)
	}
	if len(lines) != 3 {
		t.Fatalf("want 3 records (2 msgs + result), got %d", len(lines))
	}
	last := lines[2]
	if last["type"] != "result" {
		t.Errorf("final record must be type=result, got %v", last["type"])
	}
	u, _ := last["usage"].(map[string]any)
	if u["input_tokens"].(float64) != 100 || u["output_tokens"].(float64) != 42 {
		t.Errorf("result usage totals wrong: %v", u)
	}
}
