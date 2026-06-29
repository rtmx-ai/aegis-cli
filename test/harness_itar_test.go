package offline

import (
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// TestHarnessAirgapITAR → REQ-OC-018: the rebranded OpenCode is ITAR/air-gap-safe — only
// whitelisted models (OC-012), no egress beyond loopback, and no reachable cloud provider. The
// hardened launch env forbids external provider/plugin fetch (OPENCODE_PURE) + models.dev fetch,
// the shipped config exposes only a loopback provider, and the live egress gate holds.
func TestHarnessAirgapITAR(t *testing.T) {
	root := repoRoot(t)
	// 1. Launch env: no external provider/plugin fetch (so a hand-configured cloud SDK can't
	//    load), no models.dev fetch, no share/telemetry.
	srv := readRepoFile(t, "internal/opencode/serve.go")
	for _, want := range []string{"OPENCODE_PURE=1", "OPENCODE_DISABLE_MODELS_FETCH=1", "OPENCODE_DISABLE_SHARE=1"} {
		if !strings.Contains(srv, want) {
			t.Errorf("airgapEnv must harden against cloud: missing %q", want)
		}
	}
	// 2. Shipped config + whitelist expose no cloud endpoint.
	for _, pair := range []struct{ file string }{{"deploy/opencode/opencode.json"}, {"deploy/opencode/models-whitelist.json"}} {
		body := readRepoFile(t, pair.file)
		for _, cloud := range []string{"api.anthropic.com", "api.openai.com", "googleapis.com", "api.x.ai", "\"anthropic\"", "\"openai\""} {
			if strings.Contains(body, cloud) {
				t.Errorf("%s must expose no cloud endpoint/provider: found %q", pair.file, cloud)
			}
		}
	}
	// 3. Gated live gate: opencode bootstraps under verify-airgap with EGRESS=0.
	if isExecFile(filepath.Join(root, "bin", "aegis")) && isExecFile(filepath.Join(root, "deploy", "opencode", "bin", "opencode")) {
		cmd := exec.Command("scripts/verify-airgap.sh", "--", "./bin/aegis", "verify-env", "--check-opencode")
		cmd.Dir = root
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("ITAR gate: opencode did not bootstrap under the egress gate: %v\n%s", err, out)
		}
		if !strings.Contains(string(out), "EGRESS=0") {
			t.Errorf("ITAR gate: egress gate did not report EGRESS=0:\n%s", out)
		}
	}
}
