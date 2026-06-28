package offline

import (
	"os"
	"os/exec"
	"strings"
	"testing"
)

// TestIntegrationSmoke → REQ-BUILD-012: a full-stack integration smoke brings the stack up
// on loopback (llama-server --jinja + the pinned model + OpenCode) and drives `aegis run` on
// a tiny real task, asserting it completes under the egress gate. Gated/release-tier: skips
// unless the stack is built (make ci-full) and a model GGUF is available (deploy/models or
// $MODEL_OUT) — so a fresh checkout / CPU CI without the heavy artifacts does not fail.
func TestIntegrationSmoke(t *testing.T) {
	requireStackAndModel(t)
	root := repoRoot(t)
	cmd := exec.Command("scripts/integration-smoke.sh")
	cmd.Dir = root
	cmd.Env = os.Environ()
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("integration smoke failed: %v\n%s", err, out)
	}
	if !strings.Contains(string(out), "PASS") {
		t.Errorf("smoke did not report PASS:\n%s", out)
	}
}
