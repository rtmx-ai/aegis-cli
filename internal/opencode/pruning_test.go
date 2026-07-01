package opencode

import (
	"strings"
	"testing"
)

// TestDeterministicPruning → REQ-LONGRUN-002: the context-efficiency plugin dedupes repeated identical
// tool calls (re-reads) and elides the stale middle of a long observation stream (keep first-N +
// recent-M), deterministically (no LLM) — so opencode's expensive compaction triggers later.
func TestDeterministicPruning(t *testing.T) {
	js := pluginSeedFiles[ContextEfficiencyPluginFile]
	for _, want := range []string{
		"experimental.chat.messages.transform",
		"superseded by a later identical call", // dedupe repeated identical calls
		"elided stale observation",             // keep first-N + recent-M
		"KEEP_FIRST",
		"KEEP_RECENT",
	} {
		if !strings.Contains(js, want) {
			t.Errorf("context-efficiency plugin missing %q", want)
		}
	}
}
