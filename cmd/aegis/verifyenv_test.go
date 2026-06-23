package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestVerifyEnvReportsEgressStatus models CLI-001: `aegis verify-env` reports
// egress + traceability status. The default offline-safe config reports OK.
func TestVerifyEnvDefaultReportsOK(t *testing.T) {
	var out, errb bytes.Buffer
	code := run([]string{"verify-env"}, &out, &errb)
	if code != 0 {
		t.Fatalf("verify-env on offline-safe defaults should exit 0, got %d (%s)", code, errb.String())
	}
	if !strings.Contains(out.String(), "egress=OK") {
		t.Errorf("want egress=OK, got %q", out.String())
	}
}

func TestVerifyEnvNonLoopbackFails(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "cfg.json")
	if err := os.WriteFile(p, []byte(`{"endpoint":"http://example.com:8080"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	var out, errb bytes.Buffer
	code := run([]string{"verify-env", "--config", p}, &out, &errb)
	if code == 0 {
		t.Fatal("non-loopback endpoint must fail verify-env")
	}
}

func TestVersionAndUsage(t *testing.T) {
	var out, errb bytes.Buffer
	if code := run([]string{"version"}, &out, &errb); code != 0 {
		t.Fatalf("version exit = %d", code)
	}
	if strings.TrimSpace(out.String()) == "" {
		t.Error("version should print something")
	}
	out.Reset()
	errb.Reset()
	if code := run([]string{"bogus"}, &out, &errb); code != 2 {
		t.Fatalf("unknown command should exit 2, got %d", code)
	}
}

func TestProposeRequiresPrefix(t *testing.T) {
	var out, errb bytes.Buffer
	if code := run([]string{"propose"}, &out, &errb); code != 2 {
		t.Fatalf("propose without prefix should exit 2, got %d", code)
	}
	out.Reset()
	errb.Reset()
	if code := run([]string{"propose", "LOOP"}, &out, &errb); code != 0 {
		t.Fatalf("propose LOOP should exit 0, got %d (%s)", code, errb.String())
	}
}
