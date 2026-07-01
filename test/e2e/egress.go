package e2e

import (
	"context"
	"os/exec"
	"strings"
)

// EgressDeniedCommand builds a sandboxed argv with no network namespace (E2E-005):
// bubblewrap --unshare-net removes every network interface, so the process cannot
// egress — kernel-enforced, stronger than a post-hoc pcap check. Wrapping this
// around the full golden-set run is E2E-008 (CI wiring).
func EgressDeniedCommand(cmd ...string) []string {
	return SandboxCommand(SandboxOpts{NoNetwork: true}, cmd...)
}

// EgressCanary is a command that attempts a non-loopback network operation. Inside
// the egress-denied sandbox it must fail (proving zero egress).
func EgressCanary() []string {
	return []string{"sh", "-c", "getent hosts example.com >/dev/null 2>&1 && echo REACHED || echo BLOCKED"}
}

// RunEgressCanary runs the canary inside the egress-denied sandbox and reports
// whether egress was BLOCKED (the gate passes). Requires bubblewrap.
func RunEgressCanary(ctx context.Context) (blocked bool, err error) {
	argv := EgressDeniedCommand(EgressCanary()...)
	out, runErr := exec.CommandContext(ctx, argv[0], argv[1:]...).CombinedOutput()
	if runErr != nil {
		return false, runErr // sandbox/shell failed to launch -> inconclusive
	}
	return strings.Contains(string(out), "BLOCKED"), nil
}
