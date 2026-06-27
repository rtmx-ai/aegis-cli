package origin

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

// TestOriginPolicy → REQ-MODEL-006: the per-country policy loads + validates, and Allows
// resolves a listed entry first, then the default; an invalid disposition is rejected.
func TestOriginPolicy(t *testing.T) {
	dir := t.TempDir()
	good := filepath.Join(dir, "policy.json")
	if err := os.WriteFile(good, []byte(`{"default":"deny","countries":{"US":"allow","CN":"deny"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	p, err := LoadPolicy(good)
	if err != nil {
		t.Fatalf("LoadPolicy: %v", err)
	}
	if !p.Allows("US") {
		t.Error("US must be allowed")
	}
	if p.Allows("us") == false {
		t.Error("country match must be case-insensitive")
	}
	if p.Allows("CN") {
		t.Error("CN must be denied")
	}
	if p.Allows("FR") {
		t.Error("unlisted country must fall to the default (deny)")
	}
	if p.Allows("") {
		t.Error("empty/unknown country must fall to the default (deny)")
	}

	// AEGIS_ORIGIN_POLICY overrides the path.
	t.Setenv("AEGIS_ORIGIN_POLICY", good)
	if PolicyPath() != good {
		t.Errorf("PolicyPath() = %q, want %q", PolicyPath(), good)
	}

	// Invalid disposition is rejected.
	bad := filepath.Join(dir, "bad.json")
	if err := os.WriteFile(bad, []byte(`{"default":"maybe","countries":{}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := LoadPolicy(bad); err == nil {
		t.Error("an invalid default disposition must be rejected")
	}
}

// TestOriginGate → REQ-MODEL-007: CheckModel resolves the model's origin from the catalog
// and enforces the policy — allowed origin passes, denied origin and unknown origin (under
// default-deny) return a *Denial.
func TestOriginGate(t *testing.T) {
	catalog := []byte(`{"models":[
		{"id":"gemma","name":"Gemma","file":"gemma.gguf","origin":"US"},
		{"id":"qwen","name":"Qwen","file":"qwen.gguf","origin":"CN"},
		{"id":"mystery","name":"Mystery","file":"mystery.gguf"}
	]}`)

	denyCN := &Policy{Default: Deny, Countries: map[string]string{"US": Allow, "CN": Deny}}
	if err := CheckModel("gemma.gguf", catalog, denyCN); err != nil {
		t.Errorf("US-origin model must pass: %v", err)
	}
	var d *Denial
	if err := CheckModel("qwen.gguf", catalog, denyCN); !errors.As(err, &d) || d.Country != "CN" {
		t.Errorf("CN-origin model must be denied with a typed Denial, got %v", err)
	}
	// Unknown origin under default-deny is denied; the Denial flags it unknown.
	if err := CheckModel("mystery.gguf", catalog, denyCN); !errors.As(err, &d) || d.Known {
		t.Errorf("unknown-origin model must be denied (unknown) under default-deny, got %v", err)
	}

	// A policy that allows CN lets the PRC-origin model through (the explicit override).
	allowCN := &Policy{Default: Deny, Countries: map[string]string{"US": Allow, "CN": Allow}}
	if err := CheckModel("qwen.gguf", catalog, allowCN); err != nil {
		t.Errorf("CN allowed by policy must pass: %v", err)
	}
	// Unknown origin passes only when the default is allow.
	if err := CheckModel("mystery.gguf", catalog, &Policy{Default: Allow}); err != nil {
		t.Errorf("unknown origin must pass under default-allow: %v", err)
	}
}
