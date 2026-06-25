package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
)

// TestCuratedCrossLayerVerbs → REQ-SURFACE-004: status/models/serve unify
// inventory + health across the stack.
func TestCuratedCrossLayerVerbs(t *testing.T) {
	var u bytes.Buffer
	usage(&u)
	for _, w := range []string{"status", "models", "serve"} {
		if !strings.Contains(u.String(), w) {
			t.Errorf("usage must advertise curated verb %q", w)
		}
	}

	// serve without a calibration fails with guidance (not a crash).
	var so, se bytes.Buffer
	if code := run([]string{"serve", "--calibration", "/no/such/cal.json"}, &so, &se); code != 1 {
		t.Errorf("serve without calibration should exit 1, got %d", code)
	}
	if !strings.Contains(se.String(), "calibration") {
		t.Errorf("serve must guide on missing calibration: %s", se.String())
	}

	// models + status against a live (mock) loopback endpoint.
	mock := mockmodel.New(mockmodel.Options{ModelID: "test-model"})
	defer mock.Close()
	dir := t.TempDir()
	cfg := filepath.Join(dir, "aegis.json")
	body := `{"endpoint":"` + mock.URL() + `","harness":"builtin","target":"linux-cpu","allow_egress":false}`
	if err := os.WriteFile(cfg, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	var mo, me bytes.Buffer
	if code := run([]string{"models", "--config", cfg}, &mo, &me); code != 0 {
		t.Fatalf("models exit %d: %s", code, me.String())
	}
	if !strings.Contains(mo.String(), "test-model") {
		t.Errorf("models must list the served model: %s", mo.String())
	}
	var sto, ste bytes.Buffer
	if code := run([]string{"status", "--config", cfg}, &sto, &ste); code != 0 {
		t.Fatalf("status exit %d: %s", code, ste.String())
	}
	if !strings.Contains(sto.String(), "test-model") {
		t.Errorf("status must report the reachable model: %s", sto.String())
	}
}

// TestRtmxCounts → SURFACE-004 (status unifies the rtmx backlog).
func TestRtmxCounts(t *testing.T) {
	dir := t.TempDir()
	db := filepath.Join(dir, "db.csv")
	if err := os.WriteFile(db, []byte("req_id,status\nA,COMPLETE\nB,PLANNED\nC,COMPLETE\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	total, complete, err := rtmxCounts(db)
	if err != nil || total != 3 || complete != 2 {
		t.Errorf("rtmxCounts = %d/%d err=%v, want 2/3", complete, total, err)
	}
}

// TestBuildServeCommand → SURFACE-004: the calibrated launch command is built
// from calibration.json (model + resolved llama-server).
func TestBuildServeCommand(t *testing.T) {
	dir := t.TempDir()
	cal := filepath.Join(dir, "cal.json")
	if err := os.WriteFile(cal, []byte(`{"target":"linux-cpu","threads":8,"batch":256,"ngl":0,"model":"/m/x.gguf","port":8080}`), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd, err := buildServeCommand(cal)
	if err != nil {
		t.Fatalf("buildServeCommand: %v", err)
	}
	args := strings.Join(cmd.Args, " ")
	for _, want := range []string{"--model", "/m/x.gguf", "llama-server", "127.0.0.1", "8080"} {
		if !strings.Contains(args, want) {
			t.Errorf("serve command missing %q: %s", want, args)
		}
	}
}
