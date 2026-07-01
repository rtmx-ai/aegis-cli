package serving

import (
	"encoding/json"
	"strings"
	"testing"
)

// TestGrammarConstrainedOutput → REQ-THINK-006: ObjectGrammar builds a GBNF grammar
// forcing a JSON object with the given keys, and it rides the chat request's grammar
// field (sent to llama-server) so a weak local model returns parseable output.
func TestGrammarConstrainedOutput(t *testing.T) {
	g := ObjectGrammar([]string{"status", "detail"})
	for _, want := range []string{"root ::=", `\"status\"`, `\"detail\"`, "string ::=", "ws ::="} {
		if !strings.Contains(g, want) {
			t.Errorf("grammar missing %q; got:\n%s", want, g)
		}
	}
	// Empty schema -> no constraint.
	if ObjectGrammar(nil) != "" {
		t.Error("empty key set must yield no grammar")
	}
	// The grammar rides the request's grammar field.
	b, err := json.Marshal(ChatRequest{Model: "m", Grammar: g})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), `"grammar":`) {
		t.Errorf("ChatRequest must serialize the grammar field: %s", b)
	}
	// A request with no grammar omits the field.
	b2, _ := json.Marshal(ChatRequest{Model: "m"})
	if strings.Contains(string(b2), "grammar") {
		t.Errorf("empty grammar must be omitted: %s", b2)
	}
}
