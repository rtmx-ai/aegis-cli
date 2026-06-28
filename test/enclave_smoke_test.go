package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// requireStackAndModel skips unless the full stack is built (make ci-full) and a model GGUF is
// available (deploy/models/<MODEL_REF name> or $MODEL_OUT) — the shared gate for the
// release-tier integration + enclave smokes, so a fresh checkout / CPU CI does not fail.
func requireStackAndModel(t *testing.T) {
	t.Helper()
	root := repoRoot(t)
	for _, f := range []string{"deploy/llama-server/bin/llama-server", "deploy/opencode/bin/opencode", "bin/aegis"} {
		fi, err := os.Stat(filepath.Join(root, f))
		if err != nil || fi.Mode().Perm()&0o111 == 0 {
			t.Skipf("full stack not built (%s); release-tier — run make ci-full", f)
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
}

// TestEnclaveSmoke → REQ-ENCLAVE-003: install the package to a clean prefix, then drive the
// INSTALLED aegis (resolving helpers from its libexec, REL-005/006) through the full stack to
// close a real task — all under the egress gate (EGRESS=0). Proves the packaging chain works
// end-to-end on a network-disabled host. Gated/release-tier (heavy: a real model run).
func TestEnclaveSmoke(t *testing.T) {
	requireStackAndModel(t)
	root := repoRoot(t)
	// Run the closed-host smoke UNDER the egress gate (the outer wrapper); enclave-smoke.sh
	// sets ENCLAVE_OUTER_GATE so the inner run is not double-wrapped.
	cmd := exec.Command("scripts/verify-airgap.sh", "--", "scripts/enclave-smoke.sh")
	cmd.Dir = root
	cmd.Env = os.Environ()
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("enclave smoke failed: %v\n%s", err, out)
	}
	if !strings.Contains(string(out), "PASS") {
		t.Errorf("enclave smoke did not report PASS:\n%s", out)
	}
}
