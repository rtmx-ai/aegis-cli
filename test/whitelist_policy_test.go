package offline

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestWhitelistFromPolicy → REQ-OC-013: the OpenCode model whitelist is DERIVED from aegis's
// origin policy — every whitelisted model is origin-allowed, and a denied-origin model (e.g.
// CN/Qwen under the default US-only policy) is absent. The generator + the build wiring enforce it.
func TestWhitelistFromPolicy(t *testing.T) {
	root := repoRoot(t)
	if _, err := os.Stat(filepath.Join(root, "scripts", "gen-model-whitelist.py")); err != nil {
		t.Fatalf("gen-model-whitelist.py missing: %v", err)
	}
	if !strings.Contains(readRepoFile(t, "scripts/build-opencode.sh"), "gen-model-whitelist.py") {
		t.Error("build-opencode.sh must regenerate the whitelist from policy before baking it")
	}
	var pol struct {
		Default   string            `json:"default"`
		Countries map[string]string `json:"countries"`
	}
	json.Unmarshal([]byte(readRepoFile(t, "deploy/models/origin-policy.json")), &pol)
	allowed := func(o string) bool {
		d, ok := pol.Countries[o]
		if !ok {
			d = pol.Default
		}
		return d == "allow"
	}
	var cat struct {
		Models []struct {
			ID     string `json:"id"`
			Origin string `json:"origin"`
		} `json:"models"`
	}
	json.Unmarshal([]byte(readRepoFile(t, "deploy/models/catalog.json")), &cat)
	origin := map[string]string{}
	for _, m := range cat.Models {
		origin[m.ID] = m.Origin
	}
	var wl map[string]struct {
		Models map[string]json.RawMessage `json:"models"`
	}
	json.Unmarshal([]byte(readRepoFile(t, "deploy/opencode/models-whitelist.json")), &wl)
	seen := 0
	for _, prov := range wl {
		for id := range prov.Models {
			seen++
			if !allowed(origin[id]) {
				t.Errorf("whitelist includes %q from non-allowed origin %q", id, origin[id])
			}
			if !allowed("CN") && origin[id] == "CN" {
				t.Errorf("CN-origin model %q in whitelist despite CN being denied", id)
			}
		}
	}
	if seen == 0 {
		t.Error("whitelist has no models — expected the origin-approved set")
	}
}
