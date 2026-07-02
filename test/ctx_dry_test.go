package offline

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// TestCtxWindowSingleSource → REQ-PERF-009: the context window is one knob. Every place that hardcodes a
// context value in a shipped config must equal serving.DefaultCtxSize, and no stale 16k literal may
// linger. This is the DRY guard: the v1.9.x field bug was the served window (16384) and OpenCode's
// accounting drifting apart because the number lived in many places. Runtime now resolves through
// serving.ResolveCtxSize, and this test keeps the remaining static JSON copies pinned to the one constant
// so bumping DefaultCtxSize can never silently leave a copy behind.
func TestCtxWindowSingleSource(t *testing.T) {
	want := serving.DefaultCtxSize

	// OpenCode model catalog: every "context" limit must equal the one constant.
	for _, n := range allJSONNumbers(t, "deploy/opencode/models-whitelist.json", "context") {
		if n != want {
			t.Errorf("models-whitelist.json context=%d; must equal serving.DefaultCtxSize=%d (one ctx knob)", n, want)
		}
	}
	// Model tuning catalogs (repo + embedded copy): every "num_ctx" must equal the one constant.
	for _, path := range []string{"deploy/models/catalog.json", "cmd/aegis/deploydata/catalog.json"} {
		nums := allJSONNumbers(t, path, "num_ctx")
		if len(nums) == 0 {
			t.Errorf("%s declares no num_ctx — expected per-model windows pinned to the ctx knob", path)
		}
		for _, n := range nums {
			if n != want {
				t.Errorf("%s num_ctx=%d; must equal serving.DefaultCtxSize=%d (one ctx knob)", path, n, want)
			}
		}
	}
	// The calibration template must not persist a stale/small ctx_size — that was the exact value that
	// survived an upgrade and pinned the served window. Absent (resolves to the default) or the constant.
	for _, n := range allJSONNumbers(t, "deploy/llama-server/calibration.json", "ctx_size") {
		if n != want {
			t.Errorf("calibration.json ctx_size=%d; a persisted ctx below the default is the stale-value trap — omit it or set %d", n, want)
		}
	}
	// Belt-and-suspenders: no bare 16384 literal anywhere in the shipped ctx configs.
	for _, path := range []string{
		"deploy/opencode/models-whitelist.json", "deploy/models/catalog.json",
		"cmd/aegis/deploydata/catalog.json", "deploy/llama-server/calibration.json",
	} {
		if strings.Contains(readRepoFile(t, path), "16384") {
			t.Errorf("%s still contains a stale 16384 literal — the ctx window is serving.DefaultCtxSize (%d)", path, want)
		}
	}
}

// allJSONNumbers returns every integer value bound to key anywhere in the JSON file (recursively),
// so the guard is robust to the exact catalog nesting.
func allJSONNumbers(t *testing.T, relPath, key string) []int {
	t.Helper()
	var doc any
	if err := json.Unmarshal([]byte(readRepoFile(t, relPath)), &doc); err != nil {
		t.Fatalf("%s: invalid JSON: %v", relPath, err)
	}
	var out []int
	var walk func(v any)
	walk = func(v any) {
		switch x := v.(type) {
		case map[string]any:
			for k, val := range x {
				if k == key {
					if f, ok := val.(float64); ok {
						out = append(out, int(f))
					}
				}
				walk(val)
			}
		case []any:
			for _, e := range x {
				walk(e)
			}
		}
	}
	walk(doc)
	return out
}
