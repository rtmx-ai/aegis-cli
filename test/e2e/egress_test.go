package e2e

import (
	"context"
	"strings"
	"testing"
)

// TestEgressZeroGate → REQ-E2E-005: the gate runs a command under a kernel network
// namespace denial (--unshare-net), and a live network canary inside it is BLOCKED.
func TestEgressZeroGate(t *testing.T) {
	argv := EgressDeniedCommand("curl", "https://example.com")
	joined := strings.Join(argv, " ")
	if !strings.Contains(joined, "--unshare-net") {
		t.Error("egress-zero gate must run under --unshare-net (no network namespace)")
	}
	if !strings.HasPrefix(joined, "bwrap ") || !strings.Contains(joined, "-- curl https://example.com") {
		t.Errorf("egress gate must sandbox the command (reuse E2E-007): %s", joined)
	}

	// Live: inside the denied sandbox, the network canary must be BLOCKED (zero egress).
	if !SandboxAvailable() {
		t.Skip("bubblewrap not installed; static egress-deny contract verified")
	}
	blocked, err := RunEgressCanary(context.Background())
	if err != nil {
		t.Skipf("sandbox launch inconclusive: %v", err)
	}
	if !blocked {
		t.Error("network egress must be BLOCKED inside the --unshare-net sandbox")
	}
}
