package e2e

import (
	"encoding/json"
	"os/exec"
	"strings"
)

// Scanner is one offline security tool in the E2E-006 stack.
type Scanner struct {
	Name string
	Bin  string
	Args []string
	Role string // secrets | vuln | sbom | sast
}

// SecurityScanners is the locked offline security stack (E2E-006): secrets, Go
// vulns, SBOM, SAST — each runs without network.
var SecurityScanners = []Scanner{
	{"gitleaks", "gitleaks", []string{"detect", "--no-git", "--no-banner", "--report-format", "json", "--report-path", "/dev/stdout"}, "secrets"},
	{"govulncheck", "govulncheck", []string{"-json", "./..."}, "vuln"},
	{"syft", "syft", []string{"dir:.", "-o", "json", "-q"}, "sbom"},
	{"gosec", "gosec", []string{"-quiet", "-fmt=json", "./..."}, "sast"},
}

// ScannerResult is a scanner's outcome.
type ScannerResult struct {
	Name     string
	Ran      bool // false when the tool isn't installed
	Findings int
}

// ParseGitleaks counts secret findings in gitleaks JSON output (a JSON array).
func ParseGitleaks(out string) int {
	var findings []map[string]any
	if json.Unmarshal([]byte(strings.TrimSpace(out)), &findings) != nil {
		return 0
	}
	return len(findings)
}

// ParseGosec counts SAST issues in gosec JSON output ({"Issues":[...]}).
func ParseGosec(out string) int {
	var r struct {
		Issues []map[string]any `json:"Issues"`
	}
	if json.Unmarshal([]byte(out), &r) != nil {
		return 0
	}
	return len(r.Issues)
}

// SecurityGatePasses reports whether no scanner that ran found anything (E2E-006).
// An absent scanner (Ran=false) is not a finding — CI requires the tools present.
func SecurityGatePasses(results []ScannerResult) bool {
	for _, r := range results {
		if r.Ran && r.Findings > 0 {
			return false
		}
	}
	return true
}

// ScannerAvailable reports whether a scanner's binary is installed on PATH.
func ScannerAvailable(s Scanner) bool {
	_, err := exec.LookPath(s.Bin)
	return err == nil
}
