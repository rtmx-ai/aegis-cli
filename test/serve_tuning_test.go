package offline

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestCatalogCarriesTuning → REQ-SERVE-020 (data): the shipped model catalog carries
// per-model tuning matched by the operator's Ollama tag — so a real `aegis run`
// auto-applies it. Guards against catalog drift (e.g. dropping the `ollama` tag).
func TestCatalogCarriesTuning(t *testing.T) {
	b, err := os.ReadFile(filepath.Join(repoRoot(t), "deploy", "models", "catalog.json"))
	if err != nil {
		t.Fatalf("read catalog: %v", err)
	}
	for _, tag := range []string{"qwen3-coder:30b", "qwen2.5-coder:14b", "gemma4-qat:32k"} {
		tn := config.TuningForModel(tag, b)
		if tn == nil {
			t.Errorf("catalog must carry tuning for %q (ollama-tag match)", tag)
			continue
		}
		if tn.NumCtx == nil || *tn.NumCtx < 16384 {
			t.Errorf("%q tuning must set num_ctx>=16384 (avoid the Ollama 4k truncation), got %+v", tag, tn.NumCtx)
		}
	}
}
