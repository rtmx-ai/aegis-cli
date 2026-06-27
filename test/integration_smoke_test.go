package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// TestIntegrationSmoke → REQ-BUILD-012: a full-stack integration smoke brings the stack up
// on loopback (llama-server --jinja + the pinned model + OpenCode) and drives `aegis run` on
// a tiny real task, asserting it completes under the egress gate. Gated/release-tier: skips
// unless the stack is built (make ci-full) and a model GGUF is available (deploy/models or
// $MODEL_OUT) — so a fresh checkout / CPU CI without the heavy artifacts does not fail.
func TestIntegrationSmoke(t *testing.T) {
	root := repoRoot(t)
	for _, f := range []string{"deploy/llama-server/bin/llama-server", "deploy/opencode/bin/opencode", "bin/aegis"} {
		fi, err := os.Stat(filepath.Join(root, f))
		if err != nil || fi.Mode().Perm()&0o111 == 0 {
			t.Skipf("full stack not built (%s); BUILD-012 is release-tier — run make ci-full", f)
		}
	}
	ref, err := os.ReadFile(filepath.Join(root, "deploy", "models", "MODEL_REF"))
	if err != nil {
		t.Skip("no MODEL_REF")
	}
	name := ""
	for _, ln := range strings.Split(string(ref), "\n") {
		if strings.Contains(ln, "\"name\"") {
			name = strings.Trim(strings.SplitN(ln, ":", 2)[1], " \t\",")
		}
	}
	staged := os.Getenv("MODEL_OUT")
	if staged == "" {
		staged = filepath.Join(root, "deploy", "models", name)
	}
	if _, err := os.Stat(staged); err != nil {
		t.Skipf("model GGUF not available (%s); stage it or set MODEL_OUT", staged)
	}
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
