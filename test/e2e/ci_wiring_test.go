package e2e

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 6; i++ {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		dir = filepath.Dir(dir)
	}
	t.Fatal("repo root (go.mod) not found")
	return ""
}

func readRepoFile(t *testing.T, rel string) string {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(repoRoot(t), rel))
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func lineWithPrefix(s, prefix string) string {
	for _, ln := range strings.Split(s, "\n") {
		if strings.HasPrefix(ln, prefix) {
			return ln
		}
	}
	return ""
}

// TestE2ECIWiring → REQ-E2E-008: the suite is wired as staged CI gates — the make ci
// chain stages the gates (including the new E2E-006 security gate) in order, a make
// security target exists, and the pipeline declares the stages + the security hard gate.
func TestE2ECIWiring(t *testing.T) {
	mk := readRepoFile(t, "Makefile")
	pipe := readRepoFile(t, ".ci/pipeline.yml")

	// The ci target chains the staged gates including security.
	ci := lineWithPrefix(mk, "ci:")
	if ci == "" {
		t.Fatal("Makefile has no ci: target")
	}
	for _, stage := range []string{"build", "test", "race", "cover-gate", "vuln", "security", "airgap", "health", "metrics"} {
		if !strings.Contains(ci, " "+stage) {
			t.Errorf("make ci must stage %q: %s", stage, ci)
		}
	}
	// security follows vuln (supply-chain gates grouped) and precedes airgap.
	if !(strings.Index(ci, " security") > strings.Index(ci, " vuln") && strings.Index(ci, " security") < strings.Index(ci, " airgap")) {
		t.Errorf("security must be staged after vuln and before airgap: %s", ci)
	}
	// A make security target exists.
	if !strings.Contains(mk, "\nsecurity:") {
		t.Error("Makefile must define a security target")
	}

	// The pipeline declares the security + golden-metrics + egress stages.
	for _, want := range []string{"id: security", "id: golden-metrics", "id: airgap-gate"} {
		if !strings.Contains(pipe, want) {
			t.Errorf("pipeline.yml must declare stage %q", want)
		}
	}
	// The security gate is a declared hard gate.
	if !strings.Contains(pipe, "SECURITY=clean") {
		t.Error("pipeline.yml hard_gates must include the security gate")
	}
}
