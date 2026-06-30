package offline

import (
	"strings"
	"testing"
)

func provScreenPatch(t *testing.T) string {
	t.Helper()
	return readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
}

// TestHarnessManualProvision → REQ-OC-038: provisioning is operator-initiated (Ctrl+G) — no auto-start.
func TestHarnessManualProvision(t *testing.T) {
	p := provScreenPatch(t)
	if strings.Contains(p, "AEGIS_AUTO_PROVISION") {
		t.Error("OC-038: the screen must not auto-start the download (no AEGIS_AUTO_PROVISION)")
	}
	if !strings.Contains(p, "operator-initiated") || !strings.Contains(p, "Ctrl+G") {
		t.Error("OC-038: the manual gate (Ctrl+G, operator-initiated) must be present")
	}
}

// TestHarnessShowsModelURL → REQ-OC-039: the screen shows the download source URL.
func TestHarnessShowsModelURL(t *testing.T) {
	if !strings.Contains(provScreenPatch(t), "AEGIS_MODEL_URL") {
		t.Error("OC-039: the screen must show the download source (AEGIS_MODEL_URL)")
	}
}

// TestHarnessStartupProgress → REQ-OC-041: a distinct startup/loading phase after the download.
func TestHarnessStartupProgress(t *testing.T) {
	if !strings.Contains(provScreenPatch(t), "Loading the model into memory") {
		t.Error("OC-041: the screen must show a startup/loading phase, not just the download bar")
	}
}

// TestHarnessProvisionFailureReason → REQ-OC-044: the failure state shows the captured error.
func TestHarnessProvisionFailureReason(t *testing.T) {
	if !strings.Contains(provScreenPatch(t), "Provisioning failed:") {
		t.Error("OC-044: the failure state must show the reason (Provisioning failed: + provLine)")
	}
}

// TestHarnessUseAvailable → REQ-OC-047 (UI guard): the screen offers Ctrl+O to use a model already on
// the machine, serving its GGUF via --browse.
func TestHarnessUseAvailable(t *testing.T) {
	p := provScreenPatch(t)
	for _, want := range []string{`key: "ctrl+o"`, "AEGIS_AVAILABLE_PATH", `"--browse"`, "Ctrl+O to use it now"} {
		if !strings.Contains(p, want) {
			t.Errorf("OC-047: the use-available UI must include %q", want)
		}
	}
}
