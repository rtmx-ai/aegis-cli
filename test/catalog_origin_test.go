package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"testing"
)

// TestCatalogOriginRecorded → REQ-MODEL-005: every catalog model records a valid
// ISO-3166 alpha-2 origin so provenance is machine-checkable by the origin gate.
func TestCatalogOriginRecorded(t *testing.T) {
	b, err := os.ReadFile(filepath.Join(repoRoot(t), "deploy", "models", "catalog.json"))
	if err != nil {
		t.Fatalf("read catalog: %v", err)
	}
	var cat struct {
		Models []struct {
			ID     string `json:"id"`
			Origin string `json:"origin"`
		} `json:"models"`
	}
	if err := json.Unmarshal(b, &cat); err != nil {
		t.Fatalf("catalog malformed: %v", err)
	}
	iso := regexp.MustCompile(`^[A-Z]{2}$`)
	if len(cat.Models) == 0 {
		t.Fatal("catalog has no models")
	}
	for _, m := range cat.Models {
		if !iso.MatchString(m.Origin) {
			t.Errorf("model %q must record an ISO-3166 alpha-2 origin, got %q", m.ID, m.Origin)
		}
	}
}
