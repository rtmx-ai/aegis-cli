package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"testing"
)

var sha256Re = regexp.MustCompile(`^[0-9a-f]{64}$`)

// TestModelPinsConcrete guards the model pins (MODEL-001 / SERVE-016): MODEL_REF (the
// active default) and the catalog's switchable set (gemma-4-26b-a4b + qwen3-coder-30b-a3b)
// must each carry a concrete sha256 — not a PENDING placeholder — so stage-model.sh
// stages a verified GGUF and either model can be selected. Regression guard.
func TestModelPinsConcrete(t *testing.T) {
	var ref struct {
		Name   string `json:"name"`
		SHA256 string `json:"sha256"`
	}
	mustJSON(t, "deploy/models/MODEL_REF", &ref)
	if !sha256Re.MatchString(ref.SHA256) {
		t.Errorf("MODEL_REF sha256 must be a concrete 64-hex digest (not PENDING), got %q", ref.SHA256)
	}

	var cat struct {
		Models []struct {
			ID     string `json:"id"`
			File   string `json:"file"`
			SHA256 string `json:"sha256"`
		} `json:"models"`
	}
	mustJSON(t, "deploy/models/catalog.json", &cat)
	for _, want := range []string{"gemma-4-26b-a4b", "qwen3-coder-30b-a3b"} {
		var found bool
		for _, m := range cat.Models {
			if m.ID == want {
				found = true
				if !sha256Re.MatchString(m.SHA256) {
					t.Errorf("catalog %q must carry a concrete sha256 (switchable + stageable), got %q", want, m.SHA256)
				}
			}
		}
		if !found {
			t.Errorf("catalog must list the switchable model %q", want)
		}
	}
}

func mustJSON(t *testing.T, rel string, v any) {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(repoRoot(t), rel))
	if err != nil {
		t.Fatalf("read %s: %v", rel, err)
	}
	if err := json.Unmarshal(b, v); err != nil {
		t.Fatalf("%s malformed: %v", rel, err)
	}
}
