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

// TestVerifyEnvCheckOrigin → REQ-MODEL-007: the origin gate passes when the pinned model's
// origin is policy-allowed (shipped policy) and fails under a denying policy.
func TestVerifyEnvCheckOrigin(t *testing.T) {
	t.Chdir("../..") // repo root, where deploy/models/{MODEL_REF,catalog.json,origin-policy.json} live

	var out, errb bytes.Buffer
	if code := run([]string{"verify-env", "--check-origin"}, &out, &errb); code != 0 {
		t.Fatalf("check-origin with the shipped policy should pass, got %d (%s)", code, out.String())
	}
	if !strings.Contains(out.String(), "origin=OK") {
		t.Errorf("want origin=OK, got %q", out.String())
	}

	// A policy that denies every origin must fail the gate, whatever model is pinned.
	dir := t.TempDir()
	pol := filepath.Join(dir, "deny.json")
	if err := os.WriteFile(pol, []byte(`{"default":"deny","countries":{"US":"deny","CN":"deny"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("AEGIS_ORIGIN_POLICY", pol)
	out.Reset()
	errb.Reset()
	if code := run([]string{"verify-env", "--check-origin"}, &out, &errb); code == 0 {
		t.Errorf("a deny-all policy must fail check-origin; got exit 0 (%s)", out.String())
	}
	if !strings.Contains(out.String(), "origin=FAIL") {
		t.Errorf("want origin=FAIL, got %q", out.String())
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
