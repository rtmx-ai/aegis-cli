package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestInitDryRunWritesNothing → INSTALL: `aegis init --dry-run` runs, prints a
// plan, and writes no config file.
func TestInitDryRunWritesNothing(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "aegis.json")

	var out, errb bytes.Buffer
	code := run([]string{"init", "--dry-run", "--config", path}, &out, &errb)
	if code != 0 {
		t.Fatalf("init --dry-run exit = %d (%s)", code, errb.String())
	}
	s := out.String()
	if !strings.Contains(s, "host bootstrap plan") {
		t.Errorf("expected a plan summary, got %q", s)
	}
	if !strings.Contains(s, "dry-run: no files written") {
		t.Errorf("expected dry-run notice, got %q", s)
	}
	// Next-step guidance must be present.
	for _, want := range []string{"bench.sh", "make hooks-install", "verify-airgap.sh"} {
		if !strings.Contains(s, want) {
			t.Errorf("expected next-step guidance %q in output", want)
		}
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatal("--dry-run must not write a config file")
	}
}

// TestInitWritesValidConfig → INSTALL: `aegis init` into a temp path writes a
// valid, offline-safe config that round-trips through config.Load.
func TestInitWritesValidConfig(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "aegis.json")

	var out, errb bytes.Buffer
	code := run([]string{"init", "--config", path}, &out, &errb)
	if code != 0 {
		t.Fatalf("init exit = %d (%s)", code, errb.String())
	}
	if !strings.Contains(out.String(), "wrote config:") {
		t.Errorf("expected a wrote-config notice, got %q", out.String())
	}
	cfg, err := config.Load(path)
	if err != nil {
		t.Fatalf("written config must load: %v", err)
	}
	if cfg.AllowEgress {
		t.Error("written config must be offline-safe (AllowEgress false)")
	}
	if err := config.Validate(cfg); err != nil {
		t.Errorf("written config must validate: %v", err)
	}
}

// TestInitDoesNotClobberWithoutForce → INSTALL: re-running init over an existing
// config fails without --force and succeeds with it.
func TestInitDoesNotClobberWithoutForce(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "aegis.json")

	var out, errb bytes.Buffer
	if code := run([]string{"init", "--config", path}, &out, &errb); code != 0 {
		t.Fatalf("first init exit = %d (%s)", code, errb.String())
	}
	out.Reset()
	errb.Reset()
	if code := run([]string{"init", "--config", path}, &out, &errb); code == 0 {
		t.Fatal("second init without --force must fail (no silent clobber)")
	}
	out.Reset()
	errb.Reset()
	if code := run([]string{"init", "--force", "--config", path}, &out, &errb); code != 0 {
		t.Fatalf("init --force exit = %d (%s)", code, errb.String())
	}
}
