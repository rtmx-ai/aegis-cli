// Package bench exports a headless aegis run in intent-bench's transcript format
// (BENCH-002). intent-bench's lib/parse_transcript.py reads NDJSON where each line
// carries a `usage` block; a final {"type":"result","usage":...,"num_turns":N}
// record gives authoritative totals. We emit one record per message plus that
// final result so token/efficiency metrics are counted.
package bench

import (
	"encoding/json"
	"io"

	"github.com/rtmx-ai/aegis-cli/internal/opencode"
)

func usage(t opencode.Tokens) map[string]any {
	return map[string]any{
		"input_tokens":  int(t.Input),
		"output_tokens": int(t.Output),
	}
}

// WriteTranscript writes the messages as intent-bench NDJSON to w.
func WriteTranscript(w io.Writer, msgs []opencode.TranscriptMessage) error {
	enc := json.NewEncoder(w)
	var inTotal, outTotal int
	for _, m := range msgs {
		inTotal += int(m.Tokens.Input)
		outTotal += int(m.Tokens.Output)
		rec := map[string]any{
			"type":  m.Role,
			"usage": usage(m.Tokens),
			"content": []map[string]any{
				{"type": "text", "text": m.Text},
			},
		}
		if err := enc.Encode(rec); err != nil {
			return err
		}
	}
	// Authoritative totals record.
	return enc.Encode(map[string]any{
		"type":      "result",
		"num_turns": len(msgs),
		"usage": map[string]any{
			"input_tokens":  inTotal,
			"output_tokens": outTotal,
		},
	})
}
