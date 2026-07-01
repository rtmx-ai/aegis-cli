package e2e

import (
	"strings"
	"testing"
)

// TestSandboxedExecution → REQ-E2E-007: the bubblewrap argv locks the sandbox (no
// network, unshared namespaces, read-only system, one writable workdir) and runs the
// command after the -- separator.
func TestSandboxedExecution(t *testing.T) {
	argv := SandboxCommand(SandboxOpts{Workdir: "/w", NoNetwork: true}, "python", "run.py")
	joined := strings.Join(argv, " ")
	for _, want := range []string{
		"bwrap", "--unshare-net", "--unshare-pid", "--die-with-parent",
		"--ro-bind /usr /usr", "--bind /w /w", "--chdir /w", "-- python run.py",
	} {
		if !strings.Contains(joined, want) {
			t.Errorf("sandbox argv missing %q:\n%s", want, joined)
		}
	}

	// The network unshare is omitted only when NoNetwork is false.
	if strings.Contains(strings.Join(SandboxCommand(SandboxOpts{NoNetwork: false}, "x"), " "), "--unshare-net") {
		t.Error("--unshare-net must be omitted when NoNetwork=false")
	}
	// The command follows the -- separator.
	if argv[len(argv)-2] != "python" || argv[len(argv)-1] != "run.py" {
		t.Errorf("command must follow the -- separator: %v", argv[len(argv)-3:])
	}

	if !SandboxAvailable() {
		t.Skip("bubblewrap not installed; static sandbox contract verified")
	}
}
