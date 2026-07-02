package offline

import (
	"strconv"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// TestCatalogContextMatchesServed → REQ-PERF-008: the OpenCode model-catalog context limit must equal
// the served llama-server --ctx-size (serving.DefaultCtxSize). If they drift, OpenCode counts tokens
// against the wrong window and compacts at a fraction of the real context — the v1.9.0 "over the context
// after the first prompt" behavior, where the catalog said 16384 while the model was served at 32768.
func TestCatalogContextMatchesServed(t *testing.T) {
	wl := readRepoFile(t, "deploy/opencode/models-whitelist.json")
	want := `"context": ` + strconv.Itoa(serving.DefaultCtxSize)
	if !strings.Contains(wl, want) {
		t.Errorf("models-whitelist.json must set the model context limit to the served DefaultCtxSize (%d); a mismatch makes OpenCode compact at the wrong window:\n%s", serving.DefaultCtxSize, wl)
	}
}
