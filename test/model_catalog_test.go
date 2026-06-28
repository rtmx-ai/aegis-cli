package offline

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// TestModelCatalogNoCloud → REQ-OC-012: the built OpenCode bakes a WHITELISTED model catalog
// (no cloud/commercial providers) — the picker is fed by OPENCODE_MODELS_DEV, and the build
// sources it from deploy/opencode/models-whitelist.json (MODELS_DEV_API_JSON) instead of
// models.dev's 145-provider cloud catalog. (Stripping the inert provider-SDK strings so cloud
// can't even be manually configured is OC-018's ITAR gate.)
func TestModelCatalogNoCloud(t *testing.T) {
	root := repoRoot(t)
	// 1. The build bakes the whitelist as the catalog source, not models.dev.
	sh := readRepoFile(t, "scripts/build-opencode.sh")
	if !strings.Contains(sh, "MODELS_DEV_API_JSON") || !strings.Contains(sh, "models-whitelist.json") {
		t.Error("build-opencode.sh must bake the whitelist catalog via MODELS_DEV_API_JSON")
	}
	// 2. The whitelist itself carries no cloud/commercial providers.
	wl := readRepoFile(t, "deploy/opencode/models-whitelist.json")
	var cat map[string]json.RawMessage
	if err := json.Unmarshal([]byte(wl), &cat); err != nil {
		t.Fatalf("models-whitelist.json must be valid JSON: %v", err)
	}
	for _, cloud := range []string{"anthropic", "openai", "google", "xai", "azure", "bedrock", "vertex", "groq", "mistral"} {
		if _, ok := cat[cloud]; ok {
			t.Errorf("whitelist must not include cloud provider %q", cloud)
		}
	}
	// 3. Gated: if the binary is built, it must NOT carry the full cloud catalog — a density
	// check on a catalog-only field (the full models.dev catalog has hundreds; the whitelist ~0).
	bin := filepath.Join(root, "deploy", "opencode", "bin", "opencode")
	if fi, err := os.Stat(bin); err == nil && fi.Mode().Perm()&0o111 != 0 {
		out, _ := exec.Command("bash", "-c", "grep -c -a release_date '"+bin+"' || true").Output()
		if n, _ := strconv.Atoi(strings.TrimSpace(string(out))); n > 100 {
			t.Errorf("built opencode bakes the full cloud catalog (release_date x%d) — whitelist not applied", n)
		}
	}
}
