package e2e

import "testing"

// TestSecuritySuite → REQ-E2E-006: the locked offline stack (gitleaks/govulncheck/
// syft/gosec) is wired for all four roles, runs offline, parses findings, and the
// gate fails on any finding from a scanner that ran.
func TestSecuritySuite(t *testing.T) {
	// All four roles are covered by the locked stack.
	roles := map[string]string{}
	for _, s := range SecurityScanners {
		roles[s.Role] = s.Name
	}
	for role, want := range map[string]string{"secrets": "gitleaks", "vuln": "govulncheck", "sbom": "syft", "sast": "gosec"} {
		if roles[role] != want {
			t.Errorf("role %q: want %q, got %q", role, want, roles[role])
		}
	}

	// gitleaks runs offline (no git).
	for _, s := range SecurityScanners {
		if s.Name == "gitleaks" && !argHas(s.Args, "--no-git") {
			t.Error("gitleaks must run offline (--no-git)")
		}
	}

	// Parsers.
	if ParseGitleaks(`[{"RuleID":"aws-access-key","File":"x.go"}]`) != 1 {
		t.Error("gitleaks parse: one leak -> 1 finding")
	}
	if ParseGitleaks(`[]`) != 0 {
		t.Error("gitleaks parse: empty -> 0")
	}
	if ParseGosec(`{"Issues":[{"rule_id":"G101"},{"rule_id":"G404"}]}`) != 2 {
		t.Error("gosec parse: two issues -> 2 findings")
	}
	if ParseGosec(`{"Issues":[]}`) != 0 {
		t.Error("gosec parse: empty -> 0")
	}

	// Gate: any finding from a scanner that ran fails; ran-with-zero passes;
	// an absent scanner (Ran=false) is not counted as a finding.
	if SecurityGatePasses([]ScannerResult{{"gitleaks", true, 1}}) {
		t.Error("a finding must fail the security gate")
	}
	if !SecurityGatePasses([]ScannerResult{{"gitleaks", true, 0}, {"gosec", false, 0}}) {
		t.Error("zero findings (and an absent scanner) must pass")
	}
}

func argHas(args []string, want string) bool {
	for _, a := range args {
		if a == want {
			return true
		}
	}
	return false
}
