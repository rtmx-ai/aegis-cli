package main

import (
	"bytes"
	"net/http"
	"net/http/httptest"
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
	t.Setenv("OLLAMA_HOST", "http://127.0.0.1:1") // OC-028: no Ollama → exercise the no-model path

	launched := false
	var sawSignal string
	orig := tuiLaunch
	tuiLaunch = func(_ config.Config, _, _ string) error {
		launched = true
		sawSignal = os.Getenv("AEGIS_NO_MODEL")
		return nil
	}
	defer func() { tuiLaunch = orig }()
	origR := resolveOpencode
	resolveOpencode = func(string) (string, error) { return "opencode", nil } // CI has no built opencode binary
	defer func() { resolveOpencode = origR }()

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

// TestTUIUsesOllama → REQ-OC-028: with no local model but a running Ollama, cmdTUI launches opencode
// pointed at Ollama (endpoint + first model), not the no-model screen.
func TestTUIUsesOllama(t *testing.T) {
	root, _ := filepath.Abs("../..")
	t.Chdir(root)
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("AEGIS_CALIBRATION", "")
	t.Setenv("MODEL_DOWNLOAD_DIR", filepath.Join(home, "no-models"))
	t.Setenv("AEGIS_NO_MODEL", "")
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"models":[{"name":"llama3:8b"}]}`))
	}))
	defer srv.Close()
	t.Setenv("OLLAMA_HOST", srv.URL)

	var gotEndpoint, gotModel string
	origL := tuiLaunch
	tuiLaunch = func(c config.Config, _, _ string) error { gotEndpoint = c.Endpoint; gotModel = c.ModelID; return nil }
	defer func() { tuiLaunch = origL }()
	origR := resolveOpencode
	resolveOpencode = func(string) (string, error) { return "opencode", nil }
	defer func() { resolveOpencode = origR }()

	var o, e bytes.Buffer
	if code := cmdTUI(&o, &e); code != 0 {
		t.Fatalf("cmdTUI = %d, want 0 (stderr %s)", code, e.String())
	}
	if gotEndpoint != srv.URL || gotModel != "aegis-llama3-8b" {
		t.Errorf("cmdTUI must launch opencode against Ollama: endpoint=%q model=%q", gotEndpoint, gotModel)
	}
	if os.Getenv("AEGIS_NO_MODEL") == "1" {
		t.Error("Ollama detected -> must NOT set the no-model state")
	}
}
