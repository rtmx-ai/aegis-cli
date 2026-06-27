package offline

import (
	"os"
	"path/filepath"
	"testing"
)

// TestWholeGroupEgress → REQ-ENCLAVE-001: a full OpenCode bring-up — the bootstrap
// where the air-gap egress vectors fire (ripgrep download, plugin npm install,
// models.dev fetch) — runs under the egress gate (scripts/verify-airgap.sh: netns
// isolation, no route off-box) and observes no non-loopback peers. The whole-group
// EGRESS=0 proof for aegis + opencode.
//
// Gated: needs `unshare -rn` (the CI/enclave linux host) and a built aegis binary.
// The inner OpenCode launch check (`verify-env --check-opencode`) itself skips with a
// note when OpenCode/ripgrep are not staged, so this passes in the Go-only CI leg
// (proving aegis + the gate are clean) and enforces the full opencode bootstrap in
// `make ci-full`, where OpenCode is built. The three known per-vector regressions are
// guarded on every push by the unit tests TestRipgrepStaged (OC-009),
// TestPluginInstallSuppressed (OC-010), and TestModelsFetchDisabled (OC-011).
func TestWholeGroupEgress(t *testing.T) {
	if !netnsAvailable() && os.Getenv("AEGIS_AIRGAP_RUN") != "1" {
		t.Skip("whole-group egress proof: needs netns (unshare -rn, the CI/enclave linux host), or AEGIS_AIRGAP_RUN=1 to force the local ss-capture branch")
	}
	bin := filepath.Join(repoRoot(t), "bin", "aegis")
	if _, err := os.Stat(bin); err != nil {
		t.Skipf("aegis binary not built (%s); run `make build` first", bin)
	}
	// Run the OpenCode launch under the egress gate. Any non-loopback egress cannot
	// leave the isolated netns; a blocking-egress regression would prevent readiness
	// and fail the gate. PASS == EGRESS=0 across the group.
	if rc := runAirgap(t, bin, "verify-env", "--check-opencode"); rc != 0 {
		t.Fatalf("whole-group egress gate FAILED (rc=%d): a child egressed, or opencode could not reach readiness under netns", rc)
	}
}
