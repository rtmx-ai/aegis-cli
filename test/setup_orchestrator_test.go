package offline

import (
	"os/exec"
	"strings"
	"testing"
)

// TestSetupShimThin → REQ-SETUP-001: setup.sh is a thin shim that execs the Python
// orchestrator; no orchestration/UI logic in bash.
func TestSetupShimThin(t *testing.T) {
	s := readRepoFile(t, "setup.sh")
	if !strings.Contains(s, "python3 -m scripts.setup.main") {
		t.Error("setup.sh must exec the python orchestrator")
	}
	for _, forbidden := range []string{"build-llama.sh", "select_model", "discover_gguf", "begin "} {
		if strings.Contains(s, forbidden) {
			t.Errorf("setup.sh must be a thin shim — %q belongs in the orchestrator", forbidden)
		}
	}
	if n := strings.Count(s, "\n"); n > 30 {
		t.Errorf("setup.sh should be a thin shim (<~30 lines), got %d", n)
	}
}

// TestSetupOrchestratorModules → REQ-SETUP-002: the std-lib modules exist (UI
// isolated from steps), and the Python unittest suite passes (bridged here).
func TestSetupOrchestratorModules(t *testing.T) {
	for _, m := range []string{"orchestrator.py", "steps.py", "ui.py", "catalog.py", "profile.py", "main.py"} {
		readRepoFile(t, "scripts/setup/"+m) // fails the test if missing
	}
	if strings.Contains(readRepoFile(t, "scripts/setup/ui.py"), "subprocess") {
		t.Error("ui.py must not run steps (UI/step isolation)")
	}
	if _, err := exec.LookPath("python3"); err != nil {
		t.Skip("python3 not available; skipping the orchestrator unittest bridge")
	}
	cmd := exec.Command("python3", "-m", "unittest", "discover", "-s", "scripts/setup", "-t", ".")
	cmd.Dir = repoRoot(t)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("python setup suite failed: %v\n%s", err, out)
	}
}

// TestSetupIdempotent → REQ-SETUP-003: steps are gated on is_done (skip when done).
func TestSetupIdempotent(t *testing.T) {
	code := readRepoFile(t, "scripts/setup/orchestrator.py") + readRepoFile(t, "scripts/setup/steps.py")
	for _, want := range []string{"is_done", "already done"} {
		if !strings.Contains(code, want) {
			t.Errorf("the orchestrator must gate steps on idempotency (%q)", want)
		}
	}
}

// TestSetupRugged → REQ-SETUP-004: failures are isolated (logged, surfaced, no crash).
func TestSetupRugged(t *testing.T) {
	o := readRepoFile(t, "scripts/setup/orchestrator.py")
	for _, want := range []string{"except", "setup.log", "self.failed"} {
		if !strings.Contains(o, want) {
			t.Errorf("the orchestrator must isolate failures (%q)", want)
		}
	}
}

// TestSetupDRY → REQ-SETUP-005: steps REUSE the shell scripts (no duplicated build).
func TestSetupDRY(t *testing.T) {
	st := readRepoFile(t, "scripts/setup/steps.py")
	for _, want := range []string{"build-opencode.sh", "build-llama.sh", "stage-model.sh", "fetch-model.sh", "bench.sh", "integration-smoke.sh"} {
		if !strings.Contains(st, want) {
			t.Errorf("steps must reuse the shell script %q (DRY)", want)
		}
	}
}

// TestSetupUI → REQ-SETUP-006: the isolated UI module provides the bars + panel.
func TestSetupUI(t *testing.T) {
	u := readRepoFile(t, "scripts/setup/ui.py")
	for _, want := range []string{"def bar(", "def bounce(", "class Panel", "def tail_lines("} {
		if !strings.Contains(u, want) {
			t.Errorf("ui.py must provide %q", want)
		}
	}
}
