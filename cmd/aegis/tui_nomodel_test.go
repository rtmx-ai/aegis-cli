package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestTUINoModelLaunchesNotExits → REQ-OC-025: with no model provisioned, bare `aegis` launches the
// TUI with AEGIS_NO_MODEL set (so the operator provisions in-TUI) instead of exiting to the shell.
func TestTUINoModelLaunchesNotExits(t *testing.T) {
	root, _ := filepath.Abs("../..")
	t.Chdir(root) // opencode resolves from deploy/opencode/bin
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("AEGIS_CALIBRATION", "")
	t.Setenv("MODEL_DOWNLOAD_DIR", filepath.Join(home, "no-models"))
	t.Setenv("AEGIS_NO_MODEL", "")

	launched := false
	var sawSignal string
	orig := tuiLaunch
	tuiLaunch = func(_ config.Config, _, _ string) error {
		launched = true
		sawSignal = os.Getenv("AEGIS_NO_MODEL")
		return nil
	}
	defer func() { tuiLaunch = orig }()

	var o, e bytes.Buffer
	code := cmdTUI(&o, &e)
	if !launched {
		t.Fatalf("no-model must still launch the TUI, not exit (code %d, stderr %s)", code, e.String())
	}
	if sawSignal != "1" {
		t.Errorf("launch must carry AEGIS_NO_MODEL=1, got %q", sawSignal)
	}
	if code != 0 {
		t.Errorf("no-model launch returned %d, want 0", code)
	}
}
